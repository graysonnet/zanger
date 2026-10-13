//! Read Codex's persisted turn history through its app-server API. No model calls.
use crate::git::{Change, Comparison, Diff, parse_diff};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_MESSAGE: u64 = 32 * 1024 * 1024;

/// One owned helper process, bounded reads, and an overall deadline per request.
/// Dropping it also unblocks its reader. Never connects to or kills the user's daemon.
pub struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Result<Value, String>>,
    next_id: u64,
}

impl Client {
    pub fn start(root: &Path, config_args: &[String]) -> Result<Self, String> {
        let mut command = Command::new("codex");
        command
            .args(config_args)
            .arg("app-server")
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Cannot start Codex history reader: {e}"))?;
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, output) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                let result = reader
                    .by_ref()
                    .take(MAX_MESSAGE + 1)
                    .read_until(b'\n', &mut bytes);
                let message = match result {
                    Ok(0) => break,
                    Ok(_) if bytes.len() as u64 > MAX_MESSAGE => {
                        Err("Codex history response exceeds 32 MiB".into())
                    }
                    Ok(_) => serde_json::from_slice(&bytes)
                        .map_err(|e| format!("Invalid Codex response: {e}")),
                    Err(e) => Err(e.to_string()),
                };
                let failed = message.is_err();
                if sender.send(message).is_err() || failed {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            input: Some(input),
            output,
            next_id: 0,
        };
        client.request(
            "initialize",
            json!({
                "clientInfo": {"name":"zanger", "version":env!("CARGO_PKG_VERSION")},
                "capabilities":{"experimentalApi":true}
            }),
        )?;
        client.send(json!({"method":"initialized"}))?;
        Ok(client)
    }

    fn send(&mut self, value: Value) -> Result<(), String> {
        let input = self.input.as_mut().ok_or("Codex reader is closed")?;
        writeln!(input, "{value}")
            .and_then(|()| input.flush())
            .map_err(|e| e.to_string())
    }

    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"id":id,"method":method,"params":params}))?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let reply = self
                .output
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| format!("Codex {method} did not respond within 15 seconds"))??;
            if reply.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = reply.get("error") {
                return Err(format!(
                    "Codex {method}: {}",
                    error["message"].as_str().unwrap_or("request failed")
                ));
            }
            return reply
                .get("result")
                .cloned()
                .ok_or_else(|| "Missing Codex result".into());
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.input.take(); // EOF lets app-server close its databases cleanly.
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub root: PathBuf,
    pub marker: PathBuf,
    pub config_args: Vec<String>,
    pub initial_thread: Option<String>,
}

impl Source {
    pub fn from_environment(root: PathBuf) -> Result<Self, String> {
        Ok(Self {
            root,
            marker: std::env::var_os("ZANGER_CODEX_MARKER")
                .map(PathBuf::from)
                .ok_or("Missing Codex popup session marker; restart with zanger --codex")?,
            config_args: serde_json::from_str(
                &std::env::var("ZANGER_CODEX_CONFIG_ARGS").unwrap_or_else(|_| "[]".into()),
            )
            .map_err(|e| format!("Invalid Codex config arguments: {e}"))?,
            initial_thread: std::env::var("ZANGER_CODEX_INITIAL_THREAD")
                .ok()
                .filter(|s| !s.is_empty()),
        })
    }

    pub fn revision(&self) -> Vec<u8> {
        fs::read(&self.marker).unwrap_or_default()
    }

    pub fn load(&self) -> Result<Comparison, String> {
        let mut client = Client::start(&self.root, &self.config_args)?;
        let marker = self.revision();
        let binding: Option<Value> = if marker.is_empty() {
            None
        } else {
            Some(
                serde_json::from_slice(&marker)
                    .map_err(|e| format!("Invalid turn notification: {e}"))?,
            )
        };
        let bound_id = binding
            .as_ref()
            .and_then(|v| v["thread-id"].as_str())
            .or(self.initial_thread.as_deref());
        let (thread, origin) = if let Some(id) = bound_id {
            (client.request("thread/read", json!({"threadId":id,"includeTurns":false}))?["thread"].clone(), "Attached session")
        } else {
            let path = self.root.to_string_lossy().to_string();
            let plain = path.strip_prefix(r"\\?\").unwrap_or(&path);
            let list = client.request(
                "thread/list",
                json!({"cwd":[path,plain],"limit":1,
                "sourceKinds":["cli"],"sortKey":"updated_at","useStateDbOnly":true}),
            )?;
            let Some(first) = list["data"].as_array().and_then(|items| items.first()) else {
                return Ok(Comparison::recorded(self.root.clone(), "Last Codex turn".into(),
                    "No saved CLI conversation in this workspace yet · complete a Codex reply to begin".into(), Vec::new()));
            };
            (first.clone(), "Latest saved session")
        };
        if !same_workspace(
            &self.root,
            Path::new(thread["cwd"].as_str().unwrap_or_default()),
        ) {
            return Err("Codex session belongs to another workspace; no changes shown".into());
        }
        let id = thread["id"].as_str().ok_or("Codex thread has no ID")?;
        let expected = binding.as_ref().and_then(|v| v["turn-id"].as_str());
        let mut cursor = Value::Null;
        // Only fetch recent pages; never scan the entire conversation on every popup.
        for _ in 0..8 {
            let page = client.request(
                "thread/turns/list",
                json!({"threadId":id,"limit":8,
                "sortDirection":"desc","itemsView":"full","cursor":cursor}),
            )?;
            let turns = page["data"]
                .as_array()
                .ok_or("Codex returned no turn list; update Codex CLI")?;
            if let Some(turn) = select_turn(turns, expected) {
                return recorded_turn(&self.root, &thread, turn, origin);
            }
            cursor = page["nextCursor"].clone();
            if cursor.is_null() {
                break;
            }
        }
        if expected.is_some() {
            Err("The completed turn is not in saved history yet; press r to retry".into())
        } else {
            Ok(Comparison::recorded(
                self.root.clone(),
                "Last Codex turn".into(),
                format!("{origin} · {id} · no finished turn yet"),
                Vec::new(),
            ))
        }
    }
}

fn same_workspace(a: &Path, b: &Path) -> bool {
    fn normalize(path: &Path) -> String {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let value = path.to_string_lossy();
        let value = value
            .strip_prefix(r"\\?\")
            .unwrap_or(&value)
            .trim_end_matches(['/', '\\']);
        if cfg!(windows) {
            value.replace('/', "\\").to_lowercase()
        } else {
            value.into()
        }
    }
    normalize(a) == normalize(b)
}

fn select_turn<'a>(turns: &'a [Value], expected: Option<&str>) -> Option<&'a Value> {
    turns.iter().find(|turn| {
        if let Some(id) = expected {
            turn["id"].as_str() == Some(id)
        } else {
            matches!(
                turn["status"].as_str(),
                Some("completed" | "interrupted" | "failed")
            )
        }
    })
}

fn recorded_turn(
    root: &Path,
    thread: &Value,
    turn: &Value,
    origin: &str,
) -> Result<Comparison, String> {
    let mut files: BTreeMap<PathBuf, (Change, String)> = BTreeMap::new();
    let mut skipped = 0;
    let items = turn["items"]
        .as_array()
        .ok_or("Codex turn items are unavailable")?;
    for (index, item) in items.iter().enumerate() {
        if item["type"] != "fileChange" || item["status"] != "completed" {
            continue;
        }
        let changes = item["changes"]
            .as_array()
            .ok_or("Invalid Codex file changes")?;
        for change in changes {
            let Some(path) = change["path"].as_str() else {
                skipped += 1;
                continue;
            };
            let kind = change["kind"]["type"].as_str().unwrap_or("");
            let Some(diff) = change["diff"].as_str() else {
                skipped += 1;
                continue;
            };
            let original = relative_path(root, path);
            let moved = change["kind"]["move_path"]
                .as_str()
                .map(|p| relative_path(root, p));
            let (status, path, previous_path) = match kind {
                "add" => ("A", original, None),
                "delete" => ("D", original, None),
                "update" => {
                    if let Some(moved) = moved {
                        ("R", moved, Some(original))
                    } else {
                        ("M", original, None)
                    }
                }
                _ => {
                    skipped += 1;
                    continue;
                }
            };
            let entry = files.entry(path.clone()).or_insert_with(|| {
                (
                    Change {
                        status: status.into(),
                        path,
                        previous_path,
                        untracked: false,
                    },
                    String::new(),
                )
            });
            // Keep each recorded patch in order: repeated edits can overlap and must
            // not be presented as a fabricated net diff against today's working tree.
            entry
                .1
                .push_str(&format!("diff -- Codex edit {} ({kind})\n", index + 1));
            if matches!(kind, "add" | "delete") {
                let n = diff.lines().count();
                entry.1.push_str(&if kind == "add" {
                    format!("@@ -0,0 +1,{n} @@\n")
                } else {
                    format!("@@ -1,{n} +0,0 @@\n")
                });
                for line in diff.lines() {
                    entry.1.push(if kind == "add" { '+' } else { '-' });
                    entry.1.push_str(line);
                    entry.1.push('\n');
                }
            } else {
                entry.1.push_str(diff);
                entry.1.push('\n');
            }
        }
    }
    let id = thread["id"].as_str().unwrap_or("unknown");
    let turn_id = turn["id"].as_str().unwrap_or("unknown");
    let status = turn["status"].as_str().unwrap_or("unknown");
    let description =
        format!("{origin} · {id} · turn {turn_id} ({status}) · {skipped} unsupported changes");
    let files: Vec<(Change, Diff)> = files
        .into_values()
        .map(|(change, text)| (change, parse_diff(text.as_bytes())))
        .collect();
    Ok(Comparison::recorded(
        root.to_path_buf(),
        "Last Codex turn".into(),
        description,
        files,
    ))
}

fn relative_path(root: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    let root_text = root.to_string_lossy();
    let path_text = path.to_string_lossy();
    let root = Path::new(root_text.strip_prefix(r"\\?\").unwrap_or(&root_text));
    let path = Path::new(path_text.strip_prefix(r"\\?\").unwrap_or(&path_text));
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

/// Codex invokes this only for a completed turn. Persist identifiers, never chat text.
pub fn notify(marker: &Path, payload: &str) -> Result<(), String> {
    let event: Value = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    if event["type"] == "agent-turn-complete" {
        let thread = event["thread-id"]
            .as_str()
            .ok_or("Notification has no thread ID")?;
        let turn = event["turn-id"]
            .as_str()
            .ok_or("Notification has no turn ID")?;
        let data = json!({"thread-id":thread,"turn-id":turn,"cwd":event["cwd"]});
        // Replace atomically so readers never interpret a partial/empty marker
        // as permission to switch to another workspace conversation.
        let temporary = marker.with_extension(format!("{}.tmp", std::process::id()));
        let write =
            fs::write(&temporary, data.to_string()).and_then(|()| fs::rename(&temporary, marker));
        if write.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write.map_err(|e| e.to_string())?;
    }
    // Preserve the original per-invocation notification command, if configured.
    let original: Vec<String> = fs::read(marker.with_extension("notify.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    if let Some(program) = original.first() {
        Command::new(program)
            .args(&original[1..])
            .arg(payload)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::TestRepo;

    #[test]
    fn newest_finished_turn_is_selected_even_when_it_has_no_edits() {
        let turns = json!([
            {"id":"running","status":"inProgress"},
            {"id":"empty","status":"completed","items":[]},
            {"id":"older","status":"completed","items":[{"type":"fileChange"}]}
        ]);
        let turns = turns.as_array().unwrap();
        assert_eq!(select_turn(turns, None).unwrap()["id"], "empty");
        assert_eq!(select_turn(turns, Some("older")).unwrap()["id"], "older");
        assert!(select_turn(turns, Some("missing")).is_none());
        let comparison = recorded_turn(
            Path::new("."),
            &json!({"id":"thread"}),
            &turns[1],
            "Attached session",
        )
        .unwrap();
        assert!(comparison.changes.is_empty());
    }

    #[test]
    fn recorded_edits_survive_commits_deletion_and_exclude_failed_patches() {
        let repo = TestRepo::new();
        repo.write("a.ps1", "original\n");
        repo.commit();
        let thread = json!({"id":"test-thread"});
        let turn = json!({"id":"test-turn","status":"completed","items":[
            {"type":"fileChange","status":"completed","changes":[
                {"path":repo.root.join("a.ps1"),"kind":{"type":"update"},"diff":"@@ -1 +1 @@\n-original\n+first\n"},
                {"path":repo.root.join("added.ps1"),"kind":{"type":"add"},"diff":"new file\n"},
                {"path":repo.root.join("gone.ps1"),"kind":{"type":"delete"},"diff":"deleted text\n"},
                {"path":repo.root.join("old.ps1"),"kind":{"type":"update","move_path":repo.root.join("new.ps1")},"diff":"@@ -1 +1 @@\n-old\n+new\n"}
            ]},
            {"type":"fileChange","status":"completed","changes":[
                {"path":repo.root.join("a.ps1"),"kind":{"type":"update"},"diff":"@@ -1 +1 @@\n-first\n+second\n"}
            ]},
            {"type":"fileChange","status":"failed","changes":[{"path":"failed","kind":{"type":"add"},"diff":"bad"}]},
            {"type":"fileChange","status":"declined","changes":[{"path":"declined","kind":{"type":"add"},"diff":"bad"}]},
            {"type":"fileChange","status":"inProgress","changes":[{"path":"pending","kind":{"type":"add"},"diff":"bad"}]}
        ]});
        let comparison = recorded_turn(&repo.root, &thread, &turn, "Attached session").unwrap();
        assert_eq!(comparison.changes.len(), 4);
        repo.write("a.ps1", "unrelated subsequent edit\n");
        repo.commit();
        fs::remove_file(repo.root.join("a.ps1")).unwrap();
        let diff = comparison.diff(0).unwrap();
        assert_eq!((diff.additions, diff.deletions), (2, 2));
        assert!(diff.lines.iter().any(|l| l.text == "+second"));
        assert!(!diff.lines.iter().any(|l| l.text.contains("unrelated")));
        assert_eq!(comparison.diff(1).unwrap().additions, 1);
        assert_eq!(comparison.diff(2).unwrap().deletions, 1);
        assert_eq!(comparison.changes[3].previous_path, Some("old.ps1".into()));
    }

    #[test]
    fn notifications_store_only_turn_identity_and_workspaces_are_checked() {
        let repo = TestRepo::new();
        let marker = repo.root.join("turn.json");
        notify(&marker, &json!({"type":"agent-turn-complete","thread-id":"thread1","turn-id":"turn1",
            "cwd":repo.root,"input-messages":["private input"],"last-assistant-message":"private output"}).to_string()).unwrap();
        let data = fs::read_to_string(&marker).unwrap();
        assert!(!data.contains("private"));
        assert_eq!(
            serde_json::from_str::<Value>(&data).unwrap()["turn-id"],
            "turn1"
        );
        notify(&marker, r#"{"type":"unrelated"}"#).unwrap();
        assert_eq!(fs::read_to_string(&marker).unwrap(), data);
        assert!(notify(&marker, "invalid").is_err());
        assert!(same_workspace(&repo.root, &repo.root.join(".")));
        assert!(!same_workspace(&repo.root, &repo.root.join("other")));
    }

    #[test]
    #[ignore = "manual read-only integration check with installed Codex CLI"]
    fn installed_codex_history_is_readable() {
        let source = Source {
            root: std::env::current_dir().unwrap(),
            marker: PathBuf::from("target/no-notification.json"),
            config_args: Vec::new(),
            initial_thread: None,
        };
        let comparison = source.load().unwrap();
        assert_eq!(comparison.target, crate::git::Target::CodexTurn);
        assert!(comparison.title.contains("Last Codex turn"));
    }
}
