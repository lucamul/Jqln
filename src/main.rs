//! Jqln — a terminal-native writing studio for long-form prose.

mod app;
#[cfg(feature = "assistant")]
mod assistant;
mod book;
mod clipboard;
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
    jqln <project-dir>          open a project, creating it if absent
    jqln                        open the project in the current directory

OPTIONS
    --with-ai-assistant         enable the AI assistant panel (F9) for this run
    -h, --help                  print this help

The project directory holds jqln.toml plus a docs/ folder of Markdown.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return;
    }

    let mut with_ai = false;
    let mut positionals: Vec<&str> = Vec::new();
    for a in &args {
        match a.as_str() {
            "--with-ai-assistant" => with_ai = true,
            s if s.starts_with('-') => {
                eprintln!("jqln: unknown option {s}\n\n{USAGE}");
                std::process::exit(1);
            }
            s => positionals.push(s),
        }
    }

    #[cfg(not(feature = "assistant"))]
    if with_ai {
        eprintln!("jqln: this build has no assistant (compiled with --no-default-features)");
    }

    // Fail with a sentence rather than a panic when there is no terminal to
    // draw on, which is what happens under a pipe or in CI.
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        eprintln!("jqln: needs an interactive terminal");
        std::process::exit(1);
    }

    let explicit = !positionals.is_empty();
    let path = positionals.first().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));

    let project = match load_or_create(&path, explicit) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("jqln: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = run(project, with_ai) {
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

/// Block for the next event. While the assistant is streaming a reply, wake
/// every 60ms instead so tokens can be drained and drawn.
#[cfg(not(feature = "assistant"))]
fn next_event(_streaming: bool) -> std::io::Result<Option<Event>> {
    event::read().map(Some)
}

#[cfg(feature = "assistant")]
fn next_event(streaming: bool) -> std::io::Result<Option<Event>> {
    if !streaming {
        return event::read().map(Some);
    }
    if event::poll(std::time::Duration::from_millis(60))? {
        event::read().map(Some)
    } else {
        Ok(None)
    }
}

fn run(project: Project, with_ai: bool) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new(project);
    #[cfg(feature = "assistant")]
    {
        app.ai_available = with_ai;
    }
    #[cfg(not(feature = "assistant"))]
    let _ = with_ai;

    // Capturing the mouse takes over the terminal's own drag-to-select, so it
    // is a mode the writer can leave with F7 rather than a permanent cost.
    let mut capturing = false;
    let _ = set_mouse_capture(app.mouse, &mut capturing);

    let result = loop {
        if let Err(e) = terminal.draw(|f| ui::draw(f, &mut app)) {
            break Err(e);
        }
        #[cfg(feature = "assistant")]
        let streaming = app.assistant.busy;
        #[cfg(not(feature = "assistant"))]
        let streaming = false;
        match next_event(streaming) {
            Ok(Some(Event::Key(k))) if k.kind == KeyEventKind::Press => {
                // A keystroke clears any transient message from the last action.
                app.status.clear();
                app.on_key(k);
                // A copy request is fulfilled here, where stdout is reachable.
                if let Some(text) = app.clipboard.take() {
                    clipboard::copy(&text);
                }
            }
            Ok(Some(Event::Mouse(m))) => app.on_mouse(m),
            Ok(Some(_)) => {}
            // A timed-out poll: nothing to do beyond letting the assistant
            // drain below and the frame redraw at the top of the loop.
            Ok(None) => {}
            Err(e) => break Err(e),
        }
        #[cfg(feature = "assistant")]
        app.assistant_poll();
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
