//! Application state and key dispatch.

use crate::project::{count_words, Hit, Kind, NodeId, Project, ROOT};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::widgets::ListState;
use std::collections::HashMap;
use tui_textarea::{CursorMove, CursorRenderMode, TextArea, WrapMode};

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
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    None,
    Input(Prompt),
    ConfirmDelete,
    Help,
    Results,
    Snapshots,
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
    pub hits: Vec<Hit>,
    pub hit_sel: usize,
    pub query: String,
    pub snaps: Vec<String>,
    pub snap_sel: usize,
    /// Deleting a snapshot needs a second press; it is the backup of last resort.
    pub snap_confirm: bool,
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
            hits: Vec::new(),
            hit_sel: 0,
            query: String::new(),
            snaps: Vec::new(),
            snap_sel: 0,
            snap_confirm: false,
            mouse: true,
            binder_state: ListState::default(),
            outline_state: ListState::default(),
            pane_binder: Rect::ZERO,
            pane_editor: Rect::ZERO,
            pane_outline: Rect::ZERO,
            pane_cards: Rect::ZERO,
            card_hits: Vec::new(),
            flow_hits: Vec::new(),
            flow_inner: Rect::ZERO,
            flow_span_start: 0,
            session_base,
        }
    }

    /// Cards show the immediate children of the current container, which is
    /// how a chapter reads as a row of scenes.
    pub fn cards(&self) -> Vec<NodeId> {
        let container = self.flow_container().unwrap_or_else(|| ROOT.to_string());
        self.project.children.get(&container).cloned().unwrap_or_default()
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

    fn open_snapshots(&mut self) {
        let Some(id) = self.editor_doc() else {
            self.status = "Snapshots are per document; folders have no text".into();
            return;
        };
        self.flush();
        self.snaps = self.project.list_snapshots(&id);
        self.snap_sel = 0;
        self.snap_confirm = false;
        self.modal = Modal::Snapshots;
    }

    fn run_search(&mut self, query: String) {
        self.flush();
        self.query = query.clone();
        self.hit_sel = 0;
        match self.project.search(&query) {
            Err(e) => {
                self.hits.clear();
                self.status = format!("Bad search: {e}");
                self.modal = Modal::None;
            }
            Ok(hits) if hits.is_empty() => {
                self.hits.clear();
                self.status = format!("No matches for \"{query}\"");
                self.modal = Modal::None;
            }
            Ok(hits) => {
                self.hits = hits;
                self.modal = Modal::Results;
            }
        }
    }

    /// Jump to the document holding the selected hit.
    fn goto_hit(&mut self) {
        let Some(hit) = self.hits.get(self.hit_sel).cloned() else { return };
        self.select_id(&hit.id);
        let is_text = self
            .project
            .nodes
            .get(&hit.id)
            .map(|n| n.kind == Kind::Text)
            .unwrap_or(false);
        if is_text {
            self.ensure_editor(&hit.id);
            if hit.line > 0
                && let Some(ta) = self.editors.get_mut(&hit.id) {
                    ta.move_cursor(tui_textarea::CursorMove::Jump((hit.line - 1) as u16, 0));
                }
            self.view = View::Editor;
            self.focus = Focus::Editor;
        }
        self.modal = Modal::None;
    }

    fn compile_subtree(&mut self, id: &str) {
        self.flush();
        let opts = crate::compile::Options::default();
        match crate::compile::compile_to_file(&mut self.project, Some(id), &opts) {
            Ok(p) => {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("output").to_string();
                self.status = format!("Compiled selection to {name}");
            }
            Err(e) => self.status = format!("Compile failed: {e}"),
        }
    }

    fn do_compile(&mut self) {
        self.flush();
        let opts = crate::compile::Options::default();
        match crate::compile::compile_to_file(&mut self.project, None, &opts) {
            Ok(p) => {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("output").to_string();
                self.status = format!("Compiled to {name}");
            }
            Err(e) => self.status = format!("Compile failed: {e}"),
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

    // ---- key dispatch ----------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        // Modals swallow all input.
        if self.modal != Modal::None {
            self.modal_key(key);
            return;
        }
        if self.global_key(key) {
            return;
        }
        match (self.view, self.focus) {
            (View::Editor, Focus::Editor) => self.editor_key(key),
            // The outline is a vertical list of the same rows as the binder,
            // so it shares its commands wholesale.
            (View::Corkboard, _) => self.card_key(key),
            _ => self.binder_key(key),
        }
    }

    /// Grid navigation across cards; everything else falls through to the
    /// binder so `n`, `r`, `s`, `i` and `d` behave identically on the board.
    fn card_key(&mut self, key: KeyEvent) {
        let cards = self.cards();
        if cards.is_empty() {
            self.binder_key(key);
            return;
        }
        let cur = self
            .selected_id()
            .and_then(|id| cards.iter().position(|c| *c == id))
            .unwrap_or(0);
        let cols = self.card_cols.max(1);
        let last = cards.len() - 1;

        let next = match key.code {
            KeyCode::Left => Some(cur.saturating_sub(1)),
            KeyCode::Right => Some((cur + 1).min(last)),
            KeyCode::Up | KeyCode::Char('k') => Some(cur.saturating_sub(cols)),
            KeyCode::Down | KeyCode::Char('j') => Some((cur + cols).min(last)),
            KeyCode::Home => Some(0),
            KeyCode::End => Some(last),
            _ => None,
        };
        match next {
            Some(i) => {
                let target = cards[i].clone();
                self.select_id(&target);
            }
            None => self.binder_key(key),
        }
    }

    /// Returns true when the key was consumed.
    fn global_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('s') if ctrl => {
                self.save();
                true
            }
            KeyCode::Char('f') if ctrl => {
                self.begin(Prompt::Search, "");
                true
            }
            KeyCode::Char('q') if ctrl => {
                if self.dirty {
                    self.save();
                }
                self.quit = true;
                true
            }
            KeyCode::F(1) => {
                self.modal = Modal::Help;
                true
            }
            // Some terminals bind F1 to their own help, so offer a plain key.
            KeyCode::Char('?') => {
                self.modal = Modal::Help;
                true
            }
            KeyCode::F(2) => {
                self.view = View::Editor;
                true
            }
            KeyCode::F(3) => {
                self.view = View::Corkboard;
                self.focus = Focus::Binder;
                // A folder is a container, not a card: drop onto its first
                // child so something is actually selected on the board.
                if let Some(id) = self.selected_id() {
                    let is_folder =
                        self.project.nodes.get(&id).map(|n| n.kind == Kind::Folder).unwrap_or(false);
                    if is_folder
                        && let Some(first) =
                            self.project.children.get(&id).and_then(|c| c.first()).cloned()
                        {
                            self.select_id(&first);
                        }
                }
                true
            }
            KeyCode::F(4) => {
                self.view = View::Outliner;
                self.focus = Focus::Binder;
                true
            }
            KeyCode::F(5) => {
                self.do_compile();
                true
            }
            KeyCode::F(7) => {
                self.mouse = !self.mouse;
                self.status = if self.mouse {
                    "Mouse on".into()
                } else {
                    "Mouse off — drag to select text as usual".into()
                };
                true
            }
            // Deliberately not Ctrl-E: the editor widget binds that to
            // end-of-line, which is too ingrained to steal.
            KeyCode::F(6) => {
                self.continuous = !self.continuous;
                self.scroll = 0;
                self.status = if self.continuous {
                    "Continuous mode on".into()
                } else {
                    "Continuous mode off".into()
                };
                true
            }
            _ => false,
        }
    }

    fn binder_key(&mut self, key: KeyEvent) {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let Some(id) = self.selected_id() else {
            // Empty project: only creation makes sense.
            if matches!(key.code, KeyCode::Char('n')) {
                self.begin(Prompt::NewText, "");
            } else if matches!(key.code, KeyCode::Char('f')) {
                self.begin(Prompt::NewFolder, "");
            }
            return;
        };

        match key.code {
            KeyCode::Char('K') => {
                if self.project.move_vertical(&id, -1) {
                    self.select_id(&id);
                    self.dirty = true;
                }
            }
            KeyCode::Char('J') => {
                if self.project.move_vertical(&id, 1) {
                    self.select_id(&id);
                    self.dirty = true;
                }
            }
            KeyCode::Char('>') => {
                if self.project.indent(&id) {
                    self.select_id(&id);
                    self.dirty = true;
                }
            }
            KeyCode::Char('<') => {
                if self.project.outdent(&id) {
                    self.select_id(&id);
                    self.dirty = true;
                }
            }
            KeyCode::Up | KeyCode::Char('k') if alt => {
                if self.project.move_vertical(&id, -1) {
                    self.select_id(&id);
                    self.dirty = true;
                }
            }
            KeyCode::Down | KeyCode::Char('j') if alt => {
                if self.project.move_vertical(&id, 1) {
                    self.select_id(&id);
                    self.dirty = true;
                }
            }
            KeyCode::Right if alt => {
                if self.project.indent(&id) {
                    self.select_id(&id);
                    self.dirty = true;
                }
            }
            KeyCode::Left if alt => {
                if self.project.outdent(&id) {
                    self.select_id(&id);
                    self.dirty = true;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.sel = self.sel.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.sel += 1;
                self.clamp_sel();
            }
            KeyCode::Home => self.sel = 0,
            KeyCode::End => {
                self.sel = self.rows().len().saturating_sub(1);
            }
            KeyCode::Left => {
                // Collapse, or jump to parent when already collapsed.
                let collapsed = self.project.nodes.get(&id).map(|n| n.collapsed).unwrap_or(false);
                let has_kids = !self.project.children.get(&id).map(|c| c.is_empty()).unwrap_or(true);
                if has_kids && !collapsed {
                    if let Some(n) = self.project.nodes.get_mut(&id) {
                        n.collapsed = true;
                    }
                } else {
                    let p = self.project.parent_of(&id);
                    if p != ROOT {
                        self.select_id(&p);
                    }
                }
            }
            KeyCode::Right => {
                let has_kids = !self.project.children.get(&id).map(|c| c.is_empty()).unwrap_or(true);
                if has_kids {
                    if let Some(n) = self.project.nodes.get_mut(&id)
                        && n.collapsed {
                            n.collapsed = false;
                            return;
                        }
                    self.sel += 1;
                    self.clamp_sel();
                }
            }
            KeyCode::Char(' ') => {
                if let Some(n) = self.project.nodes.get_mut(&id) {
                    n.collapsed = !n.collapsed;
                }
            }
            KeyCode::Enter | KeyCode::Tab => {
                if self.editor_doc().is_some() {
                    self.view = View::Editor;
                    self.focus = Focus::Editor;
                } else if self.continuous {
                    // A folder has no text of its own, but in continuous mode
                    // its first document is the natural place to land.
                    if let Some(first) = self.continuous_docs().first().cloned() {
                        self.select_id(&first);
                        self.view = View::Editor;
                        self.focus = Focus::Editor;
                    }
                } else {
                    self.status = "Folders hold no text — press F6 for continuous mode".into();
                }
            }
            KeyCode::Char('n') => self.begin(Prompt::NewText, ""),
            KeyCode::Char('f') => self.begin(Prompt::NewFolder, ""),
            KeyCode::Char('r') => {
                let t = self.project.nodes.get(&id).map(|n| n.title.clone()).unwrap_or_default();
                self.begin(Prompt::Rename, &t);
            }
            KeyCode::Char('s') => {
                let t = self.project.nodes.get(&id).map(|n| n.synopsis.clone()).unwrap_or_default();
                self.begin(Prompt::Synopsis, &t);
            }
            KeyCode::Char('t') => {
                let v = self.project.nodes.get(&id).map(|n| n.status.clone()).unwrap_or_default();
                self.begin(Prompt::Status, &v);
            }
            KeyCode::Char('l') => {
                let v = self.project.nodes.get(&id).map(|n| n.label.clone()).unwrap_or_default();
                self.begin(Prompt::Label, &v);
            }
            KeyCode::Char('w') => {
                let v = self
                    .project
                    .nodes
                    .get(&id)
                    .map(|n| n.keywords.join(", "))
                    .unwrap_or_default();
                self.begin(Prompt::Keywords, &v);
            }
            KeyCode::Char('c') => {
                self.compile_subtree(&id);
            }
            KeyCode::Char('v') => {
                self.open_snapshots();
            }
            KeyCode::Char('i') => {
                if let Some(n) = self.project.nodes.get_mut(&id) {
                    n.include = !n.include;
                }
                self.dirty = true;
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                self.modal = Modal::ConfirmDelete;
            }
            _ => {}
        }
    }

    fn editor_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.focus = Focus::Binder;
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        // Formatting: wrap the selection (or the word under the cursor) in
        // Markdown that the editor then styles in place.
        match key.code {
            KeyCode::Char('b') if ctrl => {
                self.format_inline("**");
                return;
            }
            KeyCode::Char('i') if ctrl => {
                self.format_inline("*");
                return;
            }
            // Ctrl-I reaches most terminals as Tab. Treat Tab as italic only
            // when it would replace a selection — a literal tab is never what
            // the writer wants in the middle of a highlighted phrase.
            KeyCode::Tab if self.has_selection() => {
                self.format_inline("*");
                return;
            }
            KeyCode::Char('c') if alt => {
                self.toggle_centered();
                return;
            }
            KeyCode::Char('p') if alt => {
                self.insert_page_break();
                return;
            }
            _ => {}
        }

        // In continuous mode the flow scrolls as a whole and Ctrl-arrows step
        // between documents without leaving the editor.
        if self.continuous {
            match key.code {
                KeyCode::PageDown => {
                    self.scroll = self.scroll.saturating_add(10);
                    return;
                }
                KeyCode::PageUp => {
                    self.scroll = self.scroll.saturating_sub(10);
                    return;
                }
                KeyCode::Down | KeyCode::Up if ctrl => {
                    let docs = self.continuous_docs();
                    if let Some(cur) = self.editor_doc()
                        && let Some(i) = docs.iter().position(|d| *d == cur) {
                            let next = if key.code == KeyCode::Down {
                                (i + 1).min(docs.len().saturating_sub(1))
                            } else {
                                i.saturating_sub(1)
                            };
                            let target = docs[next].clone();
                            self.select_id(&target);
                        }
                    return;
                }
                _ => {}
            }
        }

        let Some(id) = self.editor_doc() else {
            self.focus = Focus::Binder;
            return;
        };
        self.ensure_editor(&id);
        if let Some(ta) = self.editors.get_mut(&id)
            && ta.input(key) {
                self.dirty = true;
            }
    }

    /// Is text selected in the document currently being edited?
    fn has_selection(&self) -> bool {
        self.editor_doc()
            .and_then(|id| self.editors.get(&id))
            .and_then(|ta| ta.selection_range())
            .is_some()
    }

    /// Recompute the in-place formatting highlights for one editor. Cheap
    /// enough to run on every frame; the editor keeps no styling of its own.
    pub fn restyle(&mut self, id: &str) {
        if let Some(ta) = self.editors.get_mut(id) {
            let marks = crate::markup::highlights(ta.lines());
            ta.clear_custom_highlight();
            for (range, style, priority) in marks {
                ta.custom_highlight(range, style, priority);
            }
        }
    }

    /// Toggle a `marker` (`*` or `**`) around the selection, or the word under
    /// the cursor when nothing is selected. The affected text stays selected
    /// so the same key toggles it straight back off.
    fn format_inline(&mut self, marker: &'static str) {
        let Some(id) = self.editor_doc() else { return };
        self.ensure_editor(&id);
        let Some(ta) = self.editors.get_mut(&id) else { return };

        let ((srow, scol), (erow, ecol)) = match ta.selection_range() {
            Some(range) => range,
            None => {
                let (row, col) = ta.cursor();
                let (start, end) = crate::markup::word_bounds(&ta.lines()[row], col);
                if start == end {
                    return; // cursor is not on a word
                }
                ((row, start), (row, end))
            }
        };
        if srow != erow {
            return; // inline formatting stays within one line
        }

        let line = ta.lines()[srow].clone();
        let start = crate::markup::byte_index(&line, scol);
        let end = crate::markup::byte_index(&line, ecol);
        let (new_line, kept_start, kept_end) =
            crate::markup::toggle_inline(&line, start, end, marker);

        // Swap the whole line, then put the selection back over the same text.
        ta.move_cursor(CursorMove::Jump(srow as u16, 0));
        ta.start_selection();
        ta.move_cursor(CursorMove::End);
        ta.insert_str(&new_line);

        let sel_start = crate::markup::char_index(&new_line, kept_start) as u16;
        let sel_end = crate::markup::char_index(&new_line, kept_end) as u16;
        ta.move_cursor(CursorMove::Jump(srow as u16, sel_start));
        ta.start_selection();
        ta.move_cursor(CursorMove::Jump(srow as u16, sel_end));

        self.dirty = true;
        self.restyle(&id);
    }

    /// Wrap the current line in a `::: center` / `:::` fence, or peel the
    /// fence back off when it is already there.
    fn toggle_centered(&mut self) {
        let Some(id) = self.editor_doc() else { return };
        self.ensure_editor(&id);
        let Some(ta) = self.editors.get_mut(&id) else { return };

        let (row, col) = ta.cursor();
        let mut lines: Vec<String> = ta.lines().to_vec();
        let fenced = row >= 1
            && row + 1 < lines.len()
            && lines[row - 1].trim() == crate::markup::CENTER_OPEN
            && lines[row + 1].trim() == crate::markup::CENTER_CLOSE;

        let new_row = if fenced {
            lines.remove(row + 1);
            lines.remove(row - 1);
            row - 1
        } else {
            lines.insert(row + 1, crate::markup::CENTER_CLOSE.to_string());
            lines.insert(row, crate::markup::CENTER_OPEN.to_string());
            row + 1
        };

        ta.select_all();
        ta.insert_str(lines.join("\n"));
        ta.move_cursor(CursorMove::Jump(new_row as u16, col as u16));

        self.dirty = true;
        self.restyle(&id);
    }

    /// Drop a `\newpage` marker on its own line after the current one.
    fn insert_page_break(&mut self) {
        let Some(id) = self.editor_doc() else { return };
        self.ensure_editor(&id);
        let Some(ta) = self.editors.get_mut(&id) else { return };
        ta.move_cursor(CursorMove::End);
        ta.insert_str(format!("\n\n{}\n", crate::markup::PAGE_BREAK));
        self.dirty = true;
        self.restyle(&id);
    }

    fn begin(&mut self, prompt: Prompt, initial: &str) {
        self.input = single_line(initial);
        self.modal = Modal::Input(prompt);
    }

    fn modal_key(&mut self, key: KeyEvent) {
        match self.modal {
            Modal::Help => {
                self.modal = Modal::None;
            }
            Modal::Results => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Up | KeyCode::Char('k') => {
                    self.hit_sel = self.hit_sel.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.hit_sel = (self.hit_sel + 1).min(self.hits.len().saturating_sub(1));
                }
                KeyCode::Enter => self.goto_hit(),
                _ => {}
            },
            Modal::Snapshots => {
                // Any key other than a second `d` cancels a pending delete.
                let was_confirming = self.snap_confirm;
                if key.code != KeyCode::Char('d') {
                    self.snap_confirm = false;
                }
                match key.code {
                KeyCode::Char('d') => {
                    let name = self.snaps.get(self.snap_sel).cloned();
                    match (was_confirming, self.editor_doc(), name) {
                        (true, Some(id), Some(name)) => {
                            self.snap_confirm = false;
                            match self.project.delete_snapshot(&id, &name) {
                                Ok(()) => {
                                    self.snaps = self.project.list_snapshots(&id);
                                    self.snap_sel =
                                        self.snap_sel.min(self.snaps.len().saturating_sub(1));
                                    self.status = "Snapshot deleted".into();
                                }
                                Err(e) => self.status = format!("Delete failed: {e}"),
                            }
                        }
                        (false, _, Some(_)) => {
                            self.snap_confirm = true;
                            self.status = "Press d again to delete this snapshot".into();
                        }
                        _ => {}
                    }
                }
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Up | KeyCode::Char('k') => {
                    self.snap_sel = self.snap_sel.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.snap_sel = (self.snap_sel + 1).min(self.snaps.len().saturating_sub(1));
                }
                KeyCode::Char('t') => {
                    if let Some(id) = self.editor_doc() {
                        self.flush();
                        match self.project.take_snapshot(&id) {
                            Ok(_) => {
                                self.snaps = self.project.list_snapshots(&id);
                                self.snap_sel = 0;
                                self.status = "Snapshot taken".into();
                            }
                            Err(e) => self.status = format!("Snapshot failed: {e}"),
                        }
                    }
                }
                KeyCode::Enter => {
                    let name = self.snaps.get(self.snap_sel).cloned();
                    if let (Some(id), Some(name)) = (self.editor_doc(), name) {
                        match self.project.restore_snapshot(&id, &name) {
                            Ok(()) => {
                                // Rebuild the editor from the restored text.
                                let body = self.project.body(&id);
                                self.editors.insert(id.clone(), prose_area(&body));
                                self.dirty = true;
                                self.status = "Restored".into();
                            }
                            Err(e) => self.status = format!("Restore failed: {e}"),
                        }
                    }
                    self.modal = Modal::None;
                }
                _ => {}
                }
            }
            Modal::ConfirmDelete => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    if let Some(id) = self.selected_id() {
                        for r in self.project.remove(&id) {
                            self.editors.remove(&r);
                        }
                        self.clamp_sel();
                        self.dirty = true;
                        self.status = "Deleted".into();
                    }
                    self.modal = Modal::None;
                }
                _ => self.modal = Modal::None,
            },
            Modal::Input(prompt) => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    let text = self.input.lines().first().cloned().unwrap_or_default();
                    // Close first, so a handler may open a modal of its own
                    // (search replaces the prompt with its result list).
                    self.modal = Modal::None;
                    self.commit(prompt, text.trim().to_string());
                }
                _ => {
                    self.input.input(key);
                }
            },
            Modal::None => {}
        }
    }

    fn commit(&mut self, prompt: Prompt, text: String) {
        match prompt {
            Prompt::NewText | Prompt::NewFolder => {
                if text.is_empty() {
                    return;
                }
                let kind = if prompt == Prompt::NewFolder { Kind::Folder } else { Kind::Text };
                // A new node lands inside an expanded folder, otherwise beside
                // the selection, so creation follows where the eye already is.
                let (parent, index) = match self.selected_id() {
                    Some(sel) => {
                        let is_open_folder = self
                            .project
                            .nodes
                            .get(&sel)
                            .map(|n| n.kind == Kind::Folder && !n.collapsed)
                            .unwrap_or(false);
                        if is_open_folder {
                            (sel.clone(), Some(0))
                        } else {
                            (self.project.parent_of(&sel), Some(self.project.index_in_parent(&sel) + 1))
                        }
                    }
                    None => (ROOT.to_string(), None),
                };
                let id = self.project.insert(&parent, index, &text, kind);
                self.select_id(&id);
                self.dirty = true;
            }
            Prompt::Rename => {
                if let Some(id) = self.selected_id()
                    && !text.is_empty() {
                        if let Some(n) = self.project.nodes.get_mut(&id) {
                            n.title = text;
                        }
                        self.dirty = true;
                    }
            }
            Prompt::Synopsis | Prompt::Status | Prompt::Label => {
                if let Some(id) = self.selected_id() {
                    if let Some(n) = self.project.nodes.get_mut(&id) {
                        match prompt {
                            Prompt::Synopsis => n.synopsis = text,
                            Prompt::Status => n.status = text,
                            _ => n.label = text,
                        }
                    }
                    self.dirty = true;
                }
            }
            Prompt::Search => self.run_search(text),
            Prompt::Keywords => {
                if let Some(id) = self.selected_id() {
                    if let Some(n) = self.project.nodes.get_mut(&id) {
                        n.keywords = text
                            .split(',')
                            .map(|k| k.trim().to_string())
                            .filter(|k| !k.is_empty())
                            .collect();
                    }
                    self.dirty = true;
                }
            }
        }
    }

    pub fn on_mouse(&mut self, ev: MouseEvent) {
        if self.modal != Modal::None {
            return; // Modals are keyboard-only.
        }
        let pos = Position { x: ev.column, y: ev.row };
        match ev.kind {
            MouseEventKind::ScrollDown => self.scroll_at(pos, 1),
            MouseEventKind::ScrollUp => self.scroll_at(pos, -1),
            MouseEventKind::Down(MouseButton::Left) => self.click_at(pos),
            _ => {}
        }
    }

    fn move_sel(&mut self, delta: isize) {
        let n = self.rows().len();
        if n == 0 {
            return;
        }
        let next = (self.sel as isize + delta).clamp(0, n as isize - 1);
        self.sel = next as usize;
    }

    fn scroll_at(&mut self, pos: Position, dir: isize) {
        match self.view {
            View::Outliner => self.move_sel(dir * 3),
            View::Corkboard => {
                if dir > 0 {
                    self.card_scroll = self.card_scroll.saturating_add(1);
                } else {
                    self.card_scroll = self.card_scroll.saturating_sub(1);
                }
            }
            View::Editor => {
                if self.pane_binder.contains(pos) {
                    self.move_sel(dir * 3);
                } else if self.continuous {
                    self.scroll = if dir > 0 {
                        self.scroll.saturating_add(3)
                    } else {
                        self.scroll.saturating_sub(3)
                    };
                } else if let Some(id) = self.editor_doc() {
                    self.ensure_editor(&id);
                    if let Some(ta) = self.editors.get_mut(&id) {
                        ta.scroll(((dir * 3) as i16, 0));
                    }
                }
            }
        }
    }

    fn click_at(&mut self, pos: Position) {
        match self.view {
            View::Outliner => {
                if self.pane_outline.contains(pos) {
                    let row = self.outline_state.offset() + (pos.y - self.pane_outline.y) as usize;
                    if row < self.rows().len() {
                        self.sel = row;
                    }
                }
            }
            View::Corkboard => {
                if let Some((id, _)) = self.card_hits.iter().find(|(_, r)| r.contains(pos)) {
                    let id = id.clone();
                    self.select_id(&id);
                }
            }
            View::Editor => {
                if self.pane_binder.contains(pos) {
                    let row = self.binder_state.offset() + (pos.y - self.pane_binder.y) as usize;
                    if row < self.rows().len() {
                        self.sel = row;
                        self.focus = Focus::Binder;
                    }
                } else if self.continuous {
                    self.click_in_flow(pos);
                } else if self.pane_editor.contains(pos) {
                    self.focus = Focus::Editor;
                    if let Some(id) = self.editor_doc() {
                        self.ensure_editor(&id);
                        if let Some(ta) = self.editors.get_mut(&id)
                            && let Some((row, col)) = ta.cursor_at_position(pos) {
                                ta.move_cursor(tui_textarea::CursorMove::Jump(
                                    row as u16,
                                    col as u16,
                                ));
                            }
                    }
                }
            }
        }
    }

    /// Continuous mode draws its documents into an offscreen buffer, so a
    /// click has to be mapped back through the same translation the blit used
    /// before the widget can hit-test it.
    fn click_in_flow(&mut self, pos: Position) {
        let Some((id, _)) = self.flow_hits.iter().find(|(_, r)| r.contains(pos)) else {
            return;
        };
        let id = id.clone();
        self.select_id(&id);
        self.focus = Focus::Editor;

        let inner = self.flow_inner;
        let offscreen = Position {
            x: pos.x.saturating_sub(inner.x),
            y: (pos.y.saturating_sub(inner.y))
                .saturating_add(self.scroll)
                .saturating_sub(self.flow_span_start),
        };
        if let Some(ta) = self.editors.get_mut(&id)
            && let Some((row, col)) = ta.cursor_at_position(offscreen) {
                ta.move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    /// A private directory per test. These run in parallel, and a clock-based
    /// name is not safe: macOS timestamp granularity is coarse enough for two
    /// tests to land on the same path, after which one test's cleanup deletes
    /// the project another is still building. A counter cannot collide.
    fn app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("jqln-app-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&dir);
        let p = Project::create(&dir, "T").unwrap();
        App::new(p)
    }

    #[test]
    fn navigates_and_opens_only_text_documents() {
        let mut a = app();
        // Row 0 is the "Manuscript" folder.
        assert_eq!(a.rows().len(), 4);
        a.on_key(key(KeyCode::Enter));
        assert!(matches!(a.focus, Focus::Binder), "folders must not open the editor");

        // Move to "Opening Scene" (row 2) and open it.
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        assert!(a.editor_doc().is_some());
        a.on_key(key(KeyCode::Enter));
        assert!(matches!(a.focus, Focus::Editor));

        // Typing lands in the document, Esc returns to the binder.
        a.on_key(key(KeyCode::Char('H')));
        a.on_key(key(KeyCode::Char('i')));
        assert_eq!(a.current_words(), 1);
        assert!(a.dirty);
        a.on_key(key(KeyCode::Esc));
        assert!(matches!(a.focus, Focus::Binder));

        // 'i' is a binder command again, not text.
        let before = a.current_words();
        a.on_key(key(KeyCode::Char('i')));
        assert_eq!(a.current_words(), before);
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn collapse_hides_children() {
        let mut a = app();
        assert_eq!(a.rows().len(), 4);
        a.on_key(key(KeyCode::Char(' ')));  // collapse "Manuscript"
        assert_eq!(a.rows().len(), 2, "collapsing must hide the subtree");
        a.on_key(key(KeyCode::Char(' ')));
        assert_eq!(a.rows().len(), 4);
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn new_document_goes_inside_an_open_folder() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('n')));
        for c in "Scene Two".chars() {
            a.on_key(key(KeyCode::Char(c)));
        }
        a.on_key(key(KeyCode::Enter));
        let id = a.selected_id().unwrap();
        assert_eq!(a.project.nodes[&id].title, "Scene Two");
        // "Manuscript" was selected and expanded, so it becomes the parent.
        let parent = a.project.parent_of(&id);
        assert_eq!(a.project.nodes[&parent].title, "Manuscript");
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn delete_needs_confirmation() {
        let mut a = app();
        let before = a.rows().len();
        a.on_key(key(KeyCode::Char('d')));
        a.on_key(key(KeyCode::Char('n')));  // anything but 'y' cancels
        assert_eq!(a.rows().len(), before);

        a.on_key(key(KeyCode::Char('d')));
        a.on_key(key(KeyCode::Char('y')));
        assert_eq!(a.rows().len(), 1, "deleting Manuscript takes its subtree");
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn alt_arrows_restructure_the_tree() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));  // "Chapter One"
        let id = a.selected_id().unwrap();
        assert_eq!(a.project.nodes[&id].title, "Chapter One");

        a.on_key(key_mod(KeyCode::Left, KeyModifiers::ALT));  // outdent
        assert_eq!(a.project.parent_of(&id), ROOT);
        // Selection follows the node it moved.
        assert_eq!(a.selected_id().as_deref(), Some(id.as_str()));

        a.on_key(key_mod(KeyCode::Right, KeyModifiers::ALT));  // indent back
        let parent = a.project.parent_of(&id);
        assert_eq!(a.project.nodes[&parent].title, "Manuscript");
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    fn type_str(a: &mut App, s: &str) {
        for c in s.chars() {
            a.on_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn metadata_fields_are_editable_and_persist() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));  // "Opening Scene"
        let id = a.selected_id().unwrap();

        a.on_key(key(KeyCode::Char('t')));
        type_str(&mut a, "First draft");
        a.on_key(key(KeyCode::Enter));

        a.on_key(key(KeyCode::Char('l')));
        type_str(&mut a, "Act One");
        a.on_key(key(KeyCode::Enter));

        a.on_key(key(KeyCode::Char('w')));
        type_str(&mut a, "salt, road,  desert ");
        a.on_key(key(KeyCode::Enter));

        assert_eq!(a.project.nodes[&id].status, "First draft");
        assert_eq!(a.project.nodes[&id].label, "Act One");
        // Comma separated, trimmed, and blanks dropped.
        assert_eq!(a.project.nodes[&id].keywords, ["salt", "road", "desert"]);

        a.save();
        let root = a.project.root.clone();
        let q = Project::open(&root).unwrap();
        assert_eq!(q.nodes[&id].keywords, ["salt", "road", "desert"]);
        assert_eq!(q.nodes[&id].status, "First draft");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn subtree_compile_writes_its_own_file() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));  // "Chapter One"
        let id = a.selected_id().unwrap();
        assert_eq!(a.project.nodes[&id].title, "Chapter One");

        // Give the scene inside some text.
        a.on_key(key(KeyCode::Down));
        let scene = a.selected_id().unwrap();
        a.ensure_editor(&scene);
        a.editors.get_mut(&scene).unwrap().insert_str("Chapter text.");

        a.on_key(key(KeyCode::Up));
        a.on_key(key(KeyCode::Char('c')));

        let path = a.project.root.join("chapter-one.md");
        assert!(path.exists(), "subtree compile should write chapter-one.md");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("Chapter text."));
        // The whole-project file is a different name, so neither clobbers the other.
        assert!(!a.project.root.join("t.md").exists());
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn letters_reach_the_editor_not_the_binder() {
        // The metadata bindings t/l/w must not fire while writing.
        let mut a = app();
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Enter));
        type_str(&mut a, "twl");
        let id = a.editor_doc().unwrap();
        assert_eq!(a.editors[&id].lines().join(""), "twl");
        assert!(matches!(a.modal, Modal::None), "no prompt should have opened");
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn search_jumps_to_the_matching_document() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        let scene = a.selected_id().unwrap();
        a.ensure_editor(&scene);
        a.editors
            .get_mut(&scene)
            .unwrap()
            .insert_str("line one\nthe kestrel turned\nline three");

        // Move away, then search from the tree.
        a.on_key(key(KeyCode::Home));
        a.on_key(key_mod(KeyCode::Char('f'), KeyModifiers::CONTROL));
        type_str(&mut a, "kestrel");
        a.on_key(key(KeyCode::Enter));
        assert!(matches!(a.modal, Modal::Results));
        assert_eq!(a.hits.len(), 1);

        a.on_key(key(KeyCode::Enter));  // jump
        assert!(matches!(a.modal, Modal::None));
        assert_eq!(a.selected_id().as_deref(), Some(scene.as_str()));
        assert!(matches!(a.focus, Focus::Editor));
        // Cursor landed on the matching line (0-based row 1).
        assert_eq!(a.editors[&scene].cursor().0, 1);
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn a_search_with_no_matches_reports_instead_of_opening_a_list() {
        let mut a = app();
        a.on_key(key_mod(KeyCode::Char('f'), KeyModifiers::CONTROL));
        type_str(&mut a, "zzzz");
        a.on_key(key(KeyCode::Enter));
        assert!(matches!(a.modal, Modal::None));
        assert!(a.status.contains("No matches"));
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn snapshot_and_restore_through_the_interface() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Enter));
        type_str(&mut a, "original");
        a.on_key(key(KeyCode::Esc));

        a.on_key(key(KeyCode::Char('v')));
        assert!(matches!(a.modal, Modal::Snapshots));
        a.on_key(key(KeyCode::Char('t')));   // take one
        assert_eq!(a.snaps.len(), 1);
        a.on_key(key(KeyCode::Esc));

        // Rewrite the document.
        a.on_key(key(KeyCode::Enter));
        let id = a.editor_doc().unwrap();
        a.editors.get_mut(&id).unwrap().select_all();
        a.editors.get_mut(&id).unwrap().insert_str("replaced");
        assert_eq!(a.editors[&id].lines().join(""), "replaced");
        a.on_key(key(KeyCode::Esc));

        // Restore brings the original text back into the live editor.
        a.on_key(key(KeyCode::Char('v')));
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.editors[&id].lines().join(""), "original");
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn snapshots_are_refused_on_folders() {
        let mut a = app();  // row 0 is a folder
        a.on_key(key(KeyCode::Char('v')));
        assert!(matches!(a.modal, Modal::None));
        assert!(a.status.contains("folders have no text"));
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn deleting_a_snapshot_takes_two_presses() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Enter));
        type_str(&mut a, "words");
        a.on_key(key(KeyCode::Esc));

        a.on_key(key(KeyCode::Char('v')));
        a.on_key(key(KeyCode::Char('t')));
        assert_eq!(a.snaps.len(), 1);

        // One press only arms it.
        a.on_key(key(KeyCode::Char('d')));
        assert_eq!(a.snaps.len(), 1, "a single press must not delete");
        assert!(a.snap_confirm);

        // Anything else disarms it.
        a.on_key(key(KeyCode::Down));
        assert!(!a.snap_confirm);
        a.on_key(key(KeyCode::Char('d')));
        a.on_key(key(KeyCode::Char('d')));
        assert!(a.snaps.is_empty(), "two presses should delete");
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn a_broken_regex_reports_instead_of_crashing() {
        let mut a = app();
        a.on_key(key_mod(KeyCode::Char('f'), KeyModifiers::CONTROL));
        type_str(&mut a, "/oops(/");
        a.on_key(key(KeyCode::Enter));
        assert!(matches!(a.modal, Modal::None));
        assert!(a.status.starts_with("Bad search:"), "got: {}", a.status);
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn ctrl_b_wraps_and_unwraps_the_selection() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Enter));
        type_str(&mut a, "the salt road");
        let id = a.editor_doc().unwrap();

        // Select "salt".
        let ta = a.editors.get_mut(&id).unwrap();
        ta.move_cursor(CursorMove::Jump(0, 4));
        ta.start_selection();
        ta.move_cursor(CursorMove::Jump(0, 8));

        a.on_key(key_mod(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert_eq!(a.editors[&id].lines().join("\n"), "the **salt** road");
        assert!(a.dirty);

        // The word stayed selected, so the same key strips it again.
        a.on_key(key_mod(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert_eq!(a.editors[&id].lines().join("\n"), "the salt road");
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn ctrl_b_with_no_selection_bolds_the_word_under_the_cursor() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Enter));
        type_str(&mut a, "hello world");
        let id = a.editor_doc().unwrap();

        a.on_key(key_mod(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert_eq!(a.editors[&id].lines().join("\n"), "hello **world**");
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn tab_italicises_a_selection_but_still_indents_otherwise() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Enter));
        type_str(&mut a, "word");
        let id = a.editor_doc().unwrap();

        // No selection: Tab inserts whitespace as before.
        a.on_key(key(KeyCode::Tab));
        assert!(a.editors[&id].lines().join("\n").ends_with("    "));

        // With a selection: Tab wraps it in italic markers.
        let ta = a.editors.get_mut(&id).unwrap();
        ta.move_cursor(CursorMove::Jump(0, 0));
        ta.start_selection();
        ta.move_cursor(CursorMove::Jump(0, 4));
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.editors[&id].lines().join("\n"), "*word*    ");
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn alt_c_toggles_a_centered_fence_and_alt_p_adds_a_page_break() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Enter));
        type_str(&mut a, "middle");
        let id = a.editor_doc().unwrap();

        a.on_key(key_mod(KeyCode::Char('c'), KeyModifiers::ALT));
        assert_eq!(a.editors[&id].lines(), ["::: center", "middle", ":::"]);

        a.on_key(key_mod(KeyCode::Char('c'), KeyModifiers::ALT));
        assert_eq!(a.editors[&id].lines(), ["middle"]);

        a.on_key(key_mod(KeyCode::Char('p'), KeyModifiers::ALT));
        assert_eq!(a.editors[&id].lines(), ["middle", "", "\\newpage", ""]);
        let _ = std::fs::remove_dir_all(&a.project.root);
    }

    #[test]
    fn edits_survive_a_save_and_reload() {
        let mut a = app();
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Enter));
        for c in "Once upon a time".chars() {
            a.on_key(key(KeyCode::Char(c)));
        }
        a.save();
        assert!(!a.dirty);
        let root = a.project.root.clone();

        let mut re = Project::open(&root).unwrap();
        assert_eq!(re.total_words(), 4);
        let _ = std::fs::remove_dir_all(&root);
    }
}
