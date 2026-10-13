//! Run Codex and a persistent Zanger popup in a dedicated Zellij session.
//! This is terminal integration; Codex plugins cannot register terminal panes.
use std::{
    env,
    error::Error,
    ffi::{OsStr, OsString},
    fs,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::{SystemTime, UNIX_EPOCH},
};

const CONFIG: &str = r#"
// Session-local bindings. Everything else passes through to Codex or Zanger.
keybinds clear-defaults=true {
    normal {
        bind "Alt z" { ToggleFloatingPanes; }
    }
}
default_mode "normal"
pane_frames true
auto_layout false
session_serialization false
show_startup_tips false
show_release_notes false
web_server false
web_sharing "disabled"
"#;

pub fn launch(args: &[String]) -> Result<(), Box<dyn Error>> {
    let (root, codex_args) = arguments(args)?;
    if env::var_os("ZELLIJ").is_some() {
        return Err("Run zanger --codex from a terminal outside Zellij to start the dedicated popup session".into());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("zanger --codex needs an interactive terminal; run it directly in PowerShell or your terminal".into());
    }
    check_program("zellij")?;
    check_program("codex")?;
    let executable = env::current_exe()?;
    let layout = layout(&root, &executable, codex_args)?;
    let files = SessionFiles::new(&layout)?;
    let config_args = config_arguments(codex_args);
    {
        let mut client = crate::codex_session::Client::start(&root, &config_args)?;
        let config = client.request(
            "config/read",
            serde_json::json!({"cwd":root,"includeLayers":false}),
        )?;
        let original = config["config"]["notify"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        fs::write(
            files.marker.with_extension("notify.json"),
            serde_json::to_vec(&original)?,
        )?;
    }
    let mut command = Command::new("zellij");
    if env::var("TERM").map_or(true, |term| term.is_empty() || term == "dumb") {
        // Zellij supplies an xterm-compatible PTY even when PowerShell doesn't
        // declare a TERM. Codex otherwise stops at a "dumb terminal" prompt.
        command.env("TERM", "xterm-256color");
    }
    let status = command
        .env("ZANGER_CODEX_MARKER", &files.marker)
        .env(
            "ZANGER_CODEX_CONFIG_ARGS",
            serde_json::to_string(&config_args)?,
        )
        .env(
            "ZANGER_CODEX_INITIAL_THREAD",
            resume_id(codex_args).unwrap_or_default(),
        )
        .args([OsStr::new("--config"), files.config.as_os_str()])
        .args([
            OsStr::new("--new-session-with-layout"),
            files.layout.as_os_str(),
        ])
        .args(["--session", &files.name])
        .current_dir(&root)
        .status()?;
    check_status(status, "Zellij")
}

fn arguments(args: &[String]) -> Result<(PathBuf, &[String]), Box<dyn Error>> {
    let split = args
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(args.len());
    let paths = &args[..split];
    if paths.len() > 1 || paths.first().is_some_and(|path| path.starts_with('-')) {
        return Err("Usage: zanger --codex [PATH] [-- CODEX_ARGS...]".into());
    }
    let root = Path::new(paths.first().map(String::as_str).unwrap_or(".")).canonicalize()?;
    if !root.is_dir() {
        return Err("PATH must be a directory".into());
    }
    let forwarded = args.get(split + 1..).unwrap_or_default();
    if forwarded.iter().any(|arg| {
        matches!(arg.as_str(), "-C" | "--cd" | "--worktree" | "--remote")
            || arg.starts_with("--cd=")
            || arg.starts_with("--remote=")
            || arg.starts_with("-C")
    }) {
        return Err("Use zanger --codex PATH for a local workspace. Forwarded --cd/-C/--worktree/--remote would separate Codex from the popup's local history".into());
    }
    Ok((root, forwarded))
}

fn check_program(program: &str) -> Result<(), Box<dyn Error>> {
    let output = Command::new(program)
        .arg("--version")
        .output()
        .map_err(|error| format!("Cannot run {program}. Install it and add it to PATH: {error}"))?;
    check_status(output.status, program)
}

fn check_status(status: ExitStatus, name: &str) -> Result<(), Box<dyn Error>> {
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} exited with {status}").into())
    }
}

fn quote(value: &OsStr) -> Result<String, Box<dyn Error>> {
    let value = value
        .to_str()
        .ok_or("Terminal layout paths and arguments must be UTF-8")?;
    if value.chars().any(char::is_control) {
        return Err("Terminal layout paths and arguments cannot contain control characters".into());
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn layout(root: &Path, executable: &Path, args: &[String]) -> Result<String, Box<dyn Error>> {
    let root = quote(root.as_os_str())?;
    let executable = quote(executable.as_os_str())?;
    let args = args
        .iter()
        .map(|arg| quote(OsStr::new(arg)))
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    Ok(format!(
        r#"layout {{
    tab name="Codex · Alt+Z files & changes" hide_floating_panes=true {{
        pane name="Codex · Alt+Z opens Zanger" command={executable} close_on_exit=true focus=true {{
            cwd {root}
            args "--codex-child" {args}
        }}
        floating_panes {{
            pane name="Zanger · Last Codex turn · Esc files · Alt+Z or q returns" command={executable} {{
                cwd {root}
                args "--codex-popup" {root}
                x "5%"
                y "5%"
                width "90%"
                height "90%"
            }}
        }}
    }}
}}
"#
    ))
}

struct SessionFiles {
    directory: PathBuf,
    config: PathBuf,
    layout: PathBuf,
    name: String,
    marker: PathBuf,
}

impl SessionFiles {
    fn new(layout: &str) -> io::Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let name = format!("zanger-codex-{}-{nonce}", std::process::id());
        let directory = env::temp_dir().join(&name);
        fs::create_dir(&directory)?;
        let files = Self {
            config: directory.join("config.kdl"),
            layout: directory.join("layout.kdl"),
            marker: directory.join("turn.json"),
            directory,
            name,
        };
        fs::write(&files.config, CONFIG)?;
        fs::write(&files.layout, layout)?;
        Ok(files)
    }
}

impl Drop for SessionFiles {
    fn drop(&mut self) {
        // Delete only files this launcher created; never recursively remove a directory.
        let _ = fs::remove_file(&self.config);
        let _ = fs::remove_file(&self.layout);
        let _ = fs::remove_file(&self.marker);
        let _ = fs::remove_file(self.marker.with_extension("notify.json"));
        let _ = fs::remove_dir(&self.directory);
    }
}

/// Called only inside the dedicated Codex pane. Closing Codex also closes its
/// tab so a hidden Zanger pane cannot leave the user in an empty session.
pub fn run_codex(args: impl IntoIterator<Item = OsString>) -> Result<(), Box<dyn Error>> {
    if env::var_os("ZELLIJ").is_none() {
        return Err("--codex-child is internal; use zanger --codex".into());
    }
    let args: Vec<_> = args.into_iter().collect();
    let source = crate::codex_session::Source::from_environment(env::current_dir()?)?;
    let notification = serde_json::json!([env::current_exe()?, "--codex-notify", source.marker]);
    let status = Command::new("codex")
        .arg("--cd")
        .arg(env::current_dir()?)
        .args(args)
        .arg("-c")
        .arg(format!("notify={notification}"))
        .status();
    if let Err(error) = &status {
        eprintln!("Could not start Codex: {error}");
    }
    let _ = Command::new("zellij")
        .args(["action", "close-tab"])
        .status();
    check_status(status?, "Codex")
}

// Only options which affect local configuration are passed to the read-only server.
fn config_arguments(args: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if matches!(arg.as_str(), "-c" | "--config" | "-p" | "--profile") {
            if let Some(value) = iter.next() {
                result.extend([arg.clone(), value.clone()]);
            }
        } else if arg.starts_with("--config=") || arg.starts_with("--profile=") {
            result.push(arg.clone());
        }
    }
    result
}

fn resume_id(args: &[String]) -> Option<String> {
    let index = args.iter().position(|arg| arg == "resume")?;
    let id = args.get(index + 1)?;
    // UUIDs are unambiguous. Picker/name/--last selection binds on completion.
    (id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')).then(|| id.clone())
}

pub fn hide_popup() -> io::Result<()> {
    let status = Command::new("zellij")
        .args(["action", "toggle-floating-panes"])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("Could not hide Zanger popup"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_keeps_arguments_literal_and_reserves_only_popup_key() {
        let layout = layout(
            Path::new("C:\\project with spaces"),
            Path::new("C:\\tools\\zanger.exe"),
            &[
                "resume".into(),
                "--last".into(),
                "quote\"; $(command)".into(),
            ],
        )
        .unwrap();
        assert!(layout.contains("hide_floating_panes=true"));
        assert!(
            layout
                .contains("args \"--codex-child\" \"resume\" \"--last\" \"quote\\\"; $(command)\"")
        );
        assert!(layout.contains("C:\\\\project with spaces"));
        assert!(layout.contains("width \"90%\""));
        assert_eq!(CONFIG.matches("bind ").count(), 1);
        assert!(CONFIG.contains("bind \"Alt z\" { ToggleFloatingPanes; }"));
        assert!(quote(OsStr::new("bad\npath")).is_err());
    }

    #[test]
    fn workspace_and_codex_arguments_cannot_diverge() {
        let root = env::current_dir().unwrap().to_string_lossy().into_owned();
        let args = vec![root.clone(), "--".into(), "resume".into(), "--last".into()];
        let (path, forwarded) = arguments(&args).unwrap();
        assert_eq!(path, env::current_dir().unwrap().canonicalize().unwrap());
        assert_eq!(forwarded, ["resume", "--last"]);
        for option in ["--cd", "--cd=elsewhere", "-Celsewhere", "--worktree"] {
            assert!(arguments(&[root.clone(), "--".into(), option.into()]).is_err());
        }
        assert!(arguments(&[root.clone(), root]).is_err());
    }

    #[test]
    fn generated_session_files_are_removed_without_touching_other_files() {
        let files = SessionFiles::new("layout { pane }\n").unwrap();
        let directory = files.directory.clone();
        let marker = directory.join("preserve.txt");
        fs::write(&marker, "keep").unwrap();
        assert!(files.config.is_file());
        drop(files);
        assert_eq!(fs::read_to_string(&marker).unwrap(), "keep");
        assert!(!directory.join("config.kdl").exists());
        fs::remove_file(marker).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
