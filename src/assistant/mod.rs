//! The AI assistant panel. State lives here; the orchestration (opening it,
//! sending, draining the stream) is in `app/assistant.rs`, and the pane is
//! drawn by `ui/assistant.rs`.

pub mod comments;
pub mod context;
pub mod cost;
pub mod keyring;
pub mod provider;

use crate::project::Project;
use context::Scope;
use provider::Event;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use tui_textarea::TextArea;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    /// A local status/help line, never sent anywhere.
    Local,
}

pub struct Turn {
    pub role: Role,
    pub text: String,
}

pub struct Assistant {
    pub open: bool,
    pub focused: bool,
    pub turns: Vec<Turn>,
    pub input: TextArea<'static>,
    /// Rows scrolled up from the bottom of the transcript.
    pub scroll_back: u16,
    pub scope: Scope,
    /// Runtime model id, seeded from `[assistant] model`; a `/model` command
    /// updates this and the project config.
    pub model: String,
    /// A request is in flight.
    pub busy: bool,
    /// The writer has agreed, this session, to send text to the provider.
    pub confirmed: bool,
    /// A message typed before the first-use confirmation, held until `/yes`.
    pub pending: Option<String>,
    /// Anchored remarks from the last reply, awaiting `/apply`.
    pub proposals: Vec<comments::Proposal>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub rx: Option<Receiver<Event>>,
    pub cancel: Arc<AtomicBool>,
}

impl Assistant {
    pub fn new(project: &Project) -> Self {
        let cfg = &project.assistant;
        Assistant {
            open: false,
            focused: false,
            turns: Vec::new(),
            input: input_area(),
            scroll_back: 0,
            scope: Scope::parse(&cfg.default_context).unwrap_or(Scope::Document),
            model: cfg.model.clone(),
            busy: false,
            confirmed: false,
            pending: None,
            proposals: Vec::new(),
            tokens_in: 0,
            tokens_out: 0,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn say(&mut self, role: Role, text: impl Into<String>) {
        self.turns.push(Turn { role, text: text.into() });
        self.scroll_back = 0;
    }

    /// The last turn, if it is a (streaming) assistant reply.
    pub fn streaming_turn(&mut self) -> Option<&mut Turn> {
        match self.turns.last_mut() {
            Some(t) if t.role == Role::Assistant => Some(t),
            _ => None,
        }
    }

    pub fn clear_input(&mut self) {
        self.input = input_area();
    }

    pub fn reset(&mut self) {
        self.turns.clear();
        self.proposals.clear();
        self.tokens_in = 0;
        self.tokens_out = 0;
        self.scroll_back = 0;
    }
}

fn input_area() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(ratatui::style::Style::default());
    ta
}
