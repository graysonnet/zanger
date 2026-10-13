use crossterm::{
    event::{
        self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
        Event, KeyEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{env, error::Error, io, path::PathBuf, process};

mod app;
mod codex_pane;
mod codex_session;
mod explorer;
mod fuzzy;
mod git;
mod git_review;
mod git_ui;
mod help;
mod mcp;
mod panes;
mod settings;
mod syntax;
mod theme;
mod todo;
mod todo_explorer;
mod todo_ui;
mod ui;
mod vim;

use app::App;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!(
        "zanger {} - A fast, read-only TUI file explorer with syntax highlighting",
        VERSION
    );
    println!();
    println!("USAGE:");
    println!("    zanger [PATH]");
    println!("    zanger --mcp");
    println!("    zanger --codex [PATH] [-- CODEX_ARGS...]");
    println!();
    println!("ARGUMENTS:");
    println!("    PATH    Directory to explore (defaults to current directory)");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help       Print this help message");
    println!("    -v, --version    Print version");
    println!("    --mcp            Serve read-only workspace tools over MCP stdio");
    println!("    --codex          Run Codex with an Alt+Z file/Git popup (requires Zellij)");
    println!();
    for section in help::SECTIONS {
        println!("{}:", section.title);
        for (key, action) in section.keys {
            println!("    {key:<22} {action}");
        }
        println!();
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--codex") => return codex_pane::launch(&args[2..]),
        Some("--codex-child") => return codex_pane::run_codex(env::args_os().skip(2)),
        Some("--codex-notify") if args.len() == 4 => {
            return codex_session::notify(std::path::Path::new(&args[2]), &args[3])
                .map_err(Into::into);
        }
        _ => {}
    }
    if args.get(1).is_some_and(|arg| arg == "--mcp") {
        if args.len() != 2 {
            return Err(
                "--mcp takes no path; each tool call requires an absolute workspace".into(),
            );
        }
        return mcp::serve(io::stdin().lock(), io::stdout().lock()).map_err(Into::into);
    }
    let mut path = PathBuf::from(".");
    let popup = args.get(1).is_some_and(|arg| arg == "--codex-popup");
    if popup && env::var_os("ZELLIJ").is_none() {
        return Err("--codex-popup is internal; use zanger --codex".into());
    }

    for arg in &args[if popup { 2 } else { 1 }..] {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                process::exit(0);
            }
            "-v" | "--version" => {
                println!("zanger {}", VERSION);
                process::exit(0);
            }
            other => {
                path = PathBuf::from(other);
                if !path.is_dir() {
                    eprintln!("Error: '{}' is not a valid directory", other);
                    process::exit(1);
                }
            }
        }
    }

    let codex_source = if popup {
        Some(codex_session::Source::from_environment(path.clone())?)
    } else {
        None
    };

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableFocusChange
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let mut app = App::new(path);
    if popup {
        app.codex_source = codex_source;
        app.open_codex_turn();
    }
    let res = run_app(&mut terminal, &mut app, popup);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableFocusChange
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    popup: bool,
) -> io::Result<()> {
    loop {
        app.poll_explorer();
        if let Some(review) = &mut app.git_review {
            review.poll();
        }
        if let Some(todos) = &mut app.todo_explorer {
            todos.poll();
        }
        terminal.draw(|f| ui::draw(f, app))?;

        if (popup
            || app.git_review.is_some()
            || app.todo_explorer.is_some()
            || app.explorer.loading())
            && !event::poll(std::time::Duration::from_millis(100))?
        {
            continue;
        }
        let ev = event::read()?;
        if popup && ev == Event::FocusGained {
            app.open_codex_turn();
        }

        if let Event::Key(key) = ev
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }

        if app.should_quit {
            if popup {
                app.should_quit = false;
                codex_pane::hide_popup()?;
                continue;
            }
            return Ok(());
        }
    }
}
