//! Jqln — a terminal-native writing studio for long-form prose.

mod app;
mod book;
mod compile;
mod markup;
mod project;
mod spell;
mod ui;

use app::App;
use crossterm::event::{self, Event, KeyEventKind};
use project::Project;
use std::path::{Path, PathBuf};

const USAGE: &str = "\
jqln — a terminal writing studio

USAGE
    jqln <project-dir>    open a project, creating it if absent
    jqln                  open the project in the current directory

The project directory holds jqln.toml plus a docs/ folder of Markdown.
";

fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next();

    if matches!(first.as_deref(), Some("-h") | Some("--help")) {
        print!("{USAGE}");
        return;
    }

    // Fail with a sentence rather than a panic when there is no terminal to
    // draw on, which is what happens under a pipe or in CI.
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        eprintln!("jqln: needs an interactive terminal");
        std::process::exit(1);
    }

    let path = first.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let explicit = std::env::args().nth(1).is_some();

    let project = match load_or_create(&path, explicit) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("jqln: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = run(project) {
        eprintln!("jqln: {e}");
        std::process::exit(1);
    }
}

fn load_or_create(path: &Path, explicit: bool) -> std::io::Result<Project> {
    if path.join("jqln.toml").exists() {
        return Project::open(path);
    }
    if !explicit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no jqln.toml here. Pass a directory to create a project: jqln my-novel",
        ));
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    Project::create(path, &name)
}

fn run(project: Project) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new(project);

    // Capturing the mouse takes over the terminal's own drag-to-select, so it
    // is a mode the writer can leave with F7 rather than a permanent cost.
    let mut capturing = false;
    let _ = set_mouse_capture(app.mouse, &mut capturing);

    let result = loop {
        if let Err(e) = terminal.draw(|f| ui::draw(f, &mut app)) {
            break Err(e);
        }
        match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => {
                // A keystroke clears any transient message from the last action.
                app.status.clear();
                app.on_key(k);
            }
            Ok(Event::Mouse(m)) => app.on_mouse(m),
            Ok(_) => {}
            Err(e) => break Err(e),
        }
        if app.mouse != capturing {
            let _ = set_mouse_capture(app.mouse, &mut capturing);
        }
        if app.quit {
            break Ok(());
        }
    };

    let _ = set_mouse_capture(false, &mut capturing);
    ratatui::restore();
    result
}

fn set_mouse_capture(on: bool, state: &mut bool) -> std::io::Result<()> {
    use crossterm::execute;
    let mut out = std::io::stdout();
    if on {
        execute!(out, crossterm::event::EnableMouseCapture)?;
    } else {
        execute!(out, crossterm::event::DisableMouseCapture)?;
    }
    *state = on;
    Ok(())
}
