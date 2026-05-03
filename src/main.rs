use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{env, error::Error, io, path::PathBuf, process};

mod app;
mod explorer;
mod syntax;
mod ui;

use app::App;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!("zanger {} - A fast, read-only TUI file explorer with syntax highlighting", VERSION);
    println!();
    println!("USAGE:");
    println!("    zanger [PATH]");
    println!();
    println!("ARGUMENTS:");
    println!("    PATH    Directory to explore (defaults to current directory)");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help       Print this help message");
    println!("    -v, --version    Print version");
    println!();
    println!("KEYBINDINGS:");
    println!("    q              Quit");
    println!("    Tab            Switch focus between file list and content pane");
    println!("    j/k or Up/Down Navigate files or scroll content");
    println!("    Enter/Space    Toggle fold/expand directory");
    println!("    za             Toggle fold/expand all directories");
    println!("    /              File name search");
    println!("    ?              Content search (ripgrep-like)");
    println!("    n/N            Jump to next/previous content match");
    println!("    PageUp/Down    Scroll content by 10 lines");
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    let mut path = PathBuf::from(".");

    for arg in &args[1..] {
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

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let mut app = App::new(path);
    let res = run_app(&mut terminal, &mut app);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
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
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let ev = event::read()?;

        if let Event::Key(key) = ev {
            if key.kind == KeyEventKind::Press {
                app.handle_key(key);
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
