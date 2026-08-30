//! Application state, plus the lifecycle glue that holds the pieces together.
//! Key dispatch lives in `input`, mouse handling in `mouse`, formatting in
//! `edit`, and the heavier actions (search, compile, snapshots) in `actions`.

mod actions;
mod book_settings;
mod edit;
mod input;
mod mouse;

pub use book_settings::BookField;

#[cfg(test)]
mod tests;

use crate::project::{count_words, Hit, Kind, NodeId, Project, ROOT};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::collections::HashMap;
use tui_textarea::{CursorRenderMode, TextArea, WrapMode};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Editor,
    Corkboard,
    Outliner,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Binder,
    Editor,
}

/// Which text field a modal is collecting, so one code path serves them all.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    NewText,
    NewFolder,
    Rename,
    Synopsis,
    Status,
    Label,
    Keywords,
    Search,
    /// The book-compile heading override for the selected folder.
    ChapterHeading,
    /// Editing one text field of the book settings.
    Book(BookField),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    None,
    Input(Prompt),
    ConfirmDelete,
    Help,
    Results,
    Snapshots,
    /// The book / PDF settings list.
    BookSettings,
}

pub struct App {
    pub project: Project,
    pub view: View,
    pub focus: Focus,
    /// Index into `project.visible()`.
    pub sel: usize,
    pub editors: HashMap<NodeId, TextArea<'static>>,
    pub modal: Modal,
    pub input: TextArea<'static>,
    pub status: String,
    pub dirty: bool,
    pub quit: bool,
    /// Continuous mode: edit every document in the current container as one
    /// scrolling flow rather than one document at a time.
    pub continuous: bool,
    /// Row offset into the continuous flow.
    pub scroll: u16,
    /// Column count of the card grid, set by the renderer so that up/down
    /// navigation can move by a row without the app guessing the layout.
    pub card_cols: usize,
    /// First visible row of the card grid.
    pub card_scroll: usize,
    /// The tree node whose children the card view is laying out.
    pub card_root: NodeId,
    pub hits: Vec<Hit>,
    pub hit_sel: usize,
    pub query: String,
    pub snaps: Vec<String>,
    pub snap_sel: usize,
    /// Deleting a snapshot needs a second press; it is the backup of last resort.
    pub snap_confirm: bool,
    /// Selected row in the book settings list.
    pub book_sel: usize,
    /// Mouse capture. While on, the terminal's own drag-to-select is
    /// suppressed, so this is toggleable rather than permanent.
    pub mouse: bool,
    /// List states live across frames so their scroll offset can be read back
    /// when translating a click into a row.
    pub binder_state: ListState,
    pub outline_state: ListState,
    pub pane_binder: Rect,
    pub pane_editor: Rect,
    pub pane_outline: Rect,
    pub pane_cards: Rect,
    pub card_hits: Vec<(NodeId, Rect)>,
    /// The card being dragged, and the card the pointer is currently over, so
    /// the corkboard can be reordered by dropping one card onto another.
    pub drag_card: Option<NodeId>,
    pub drag_over: Option<NodeId>,
    /// Screen rectangles of the documents drawn in the continuous flow.
    pub flow_hits: Vec<(NodeId, Rect)>,
    pub flow_inner: Rect,
    pub flow_span_start: u16,
    /// Total project words when the session began, for the session counter.
    pub session_base: usize,
}

/// Build a TextArea configured the way every prose surface in Jqln should be.
pub fn prose_area(body: &str) -> TextArea<'static> {
    let lines: Vec<String> = if body.is_empty() {
        vec![String::new()]
    } else {
        body.split('\n').map(|s| s.to_string()).collect()
    };
    let mut ta = TextArea::new(lines);
    ta.set_wrap_mode(WrapMode::WordOrGlyph);
    // Undo a word at a time rather than a character at a time.
    ta.set_undo_coalescing(true);
    ta.set_cursor_line_style(ratatui::style::Style::default());
    ta.set_tab_length(4);
    ta
}

fn single_line(initial: &str) -> TextArea<'static> {
    let mut ta = TextArea::new(vec![initial.to_string()]);
    ta.set_cursor_line_style(ratatui::style::Style::default());
    ta.move_cursor(tui_textarea::CursorMove::End);
    // A prompt pre-filled with the current value starts fully selected, so the
    // first keystroke replaces it; an arrow key drops the selection to edit.
    if !initial.is_empty() {
        ta.select_all();
    }
    ta
}

impl App {
    pub fn new(mut project: Project) -> Self {
        let session_base = project.total_words();
        App {
            project,
            view: View::Editor,
            focus: Focus::Binder,
            sel: 0,
            editors: HashMap::new(),
            modal: Modal::None,
            input: single_line(""),
            status: "F1 help  ·  Ctrl-S save  ·  Ctrl-Q quit".to_string(),
            dirty: false,
            quit: false,
            continuous: false,
            scroll: 0,
            card_cols: 3,
            card_scroll: 0,
            card_root: ROOT.to_string(),
            hits: Vec::new(),
            hit_sel: 0,
            query: String::new(),
            snaps: Vec::new(),
            snap_sel: 0,
            snap_confirm: false,
            book_sel: 0,
            mouse: true,
            binder_state: ListState::default(),
            outline_state: ListState::default(),
            pane_binder: Rect::ZERO,
            pane_editor: Rect::ZERO,
            pane_outline: Rect::ZERO,
            pane_cards: Rect::ZERO,
            card_hits: Vec::new(),
            drag_card: None,
            drag_over: None,
            flow_hits: Vec::new(),
            flow_inner: Rect::ZERO,
            flow_span_start: 0,
            session_base,
        }
    }

    /// Cards show the children of `card_root` — a level of the tree, so parts,
    /// chapters or scenes can be laid out and reordered as a set.
    pub fn cards(&self) -> Vec<NodeId> {
        self.project.children.get(&self.card_root).cloned().unwrap_or_default()
    }

    /// Enter the card view showing the level the selection sits on (its
    /// siblings), with the selection itself highlighted.
    pub fn enter_cards(&mut self) {
        self.card_root = match self.selected_id() {
            Some(id) => self.project.parent_of(&id),
            None => ROOT.to_string(),
        };
        self.card_scroll = 0;
        let cards = self.cards();
        let on_card = self.selected_id().map(|s| cards.contains(&s)).unwrap_or(false);
        if !on_card && let Some(first) = cards.first().cloned() {
            self.select_id(&first);
        }
    }

    /// Show the children of the highlighted folder card.
    pub fn cards_descend(&mut self) {
        let Some(sel) = self.selected_id() else { return };
        if self.project.nodes.get(&sel).map(|n| n.kind == Kind::Folder).unwrap_or(false) {
            self.card_root = sel;
            self.card_scroll = 0;
            if let Some(first) = self.cards().first().cloned() {
                self.select_id(&first);
            }
        }
    }

    /// Step back out to the parent level, keeping the folder we left highlighted.
    pub fn cards_ascend(&mut self) {
        if self.card_root == ROOT {
            return;
        }
        let left = self.card_root.clone();
        self.card_root = self.project.parent_of(&left);
        self.card_scroll = 0;
        self.select_id(&left);
    }

    /// The container whose documents make up the continuous flow: the
    /// selection itself when it is a folder, otherwise its parent, so
    /// selecting a scene shows the whole chapter around it.
    pub fn flow_container(&self) -> Option<NodeId> {
        let id = self.selected_id()?;
        let node = self.project.nodes.get(&id)?;
        if node.kind == Kind::Folder {
            Some(id)
        } else {
            let p = self.project.parent_of(&id);
            if p == ROOT { None } else { Some(p) }
        }
    }

    /// Documents shown in continuous mode, in reading order.
    pub fn continuous_docs(&self) -> Vec<NodeId> {
        match self.flow_container() {
            Some(c) => self.project.text_descendants(&c),
            None => self
                .project
                .walk()
                .into_iter()
                .map(|(i, _)| i)
                .filter(|i| {
                    self.project.nodes.get(i).map(|n| n.kind == Kind::Text).unwrap_or(false)
                })
                .collect(),
        }
    }

    pub fn rows(&self) -> Vec<(NodeId, usize)> {
        self.project.visible()
    }

    pub fn selected_id(&self) -> Option<NodeId> {
        self.rows().get(self.sel).map(|(id, _)| id.clone())
    }

    /// The document currently shown in the editor pane, if any.
    pub fn editor_doc(&self) -> Option<NodeId> {
        let id = self.selected_id()?;
        match self.project.nodes.get(&id) {
            Some(n) if n.kind == Kind::Text => Some(id),
            _ => None,
        }
    }

    pub fn ensure_editor(&mut self, id: &str) {
        if !self.editors.contains_key(id) {
            let body = self.project.body(id);
            self.editors.insert(id.to_string(), prose_area(&body));
        }
    }

    /// Copy every open editor's text back into the project model.
    pub fn flush(&mut self) {
        let updates: Vec<(NodeId, String)> = self
            .editors
            .iter()
            .map(|(id, ta)| (id.clone(), ta.lines().join("\n")))
            .collect();
        for (id, text) in updates {
            self.project.set_body(&id, text);
        }
    }

    pub fn save(&mut self) {
        self.flush();
        match self.project.save() {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("Saved to {}", self.project.root.display());
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    /// Words in the document under the cursor, read live from its editor.
    pub fn current_words(&self) -> usize {
        match self.editor_doc() {
            Some(id) => match self.editors.get(&id) {
                Some(ta) => count_words(&ta.lines().join("\n")),
                None => 0,
            },
            None => 0,
        }
    }

    pub fn total_words(&mut self) -> usize {
        self.flush();
        self.project.total_words()
    }

    fn clamp_sel(&mut self) {
        let n = self.rows().len();
        if n == 0 {
            self.sel = 0;
        } else if self.sel >= n {
            self.sel = n - 1;
        }
    }

    pub fn select_id(&mut self, id: &str) {
        if let Some(i) = self.rows().iter().position(|(n, _)| n == id) {
            self.sel = i;
        }
    }

    /// Cursor should only be painted in the pane that has focus.
    pub fn sync_cursor_modes(&mut self) {
        let active = if self.focus == Focus::Editor && self.view == View::Editor {
            self.editor_doc()
        } else {
            None
        };
        for (id, ta) in self.editors.iter_mut() {
            let mode = if Some(id.clone()) == active {
                CursorRenderMode::Cell
            } else {
                CursorRenderMode::Hidden
            };
            ta.set_cursor_render_mode(mode);
        }
    }
}
