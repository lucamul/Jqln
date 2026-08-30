//! Key dispatch: routing a keystroke to the binder, the editor, the card grid
//! or the active modal, and committing the text a prompt collected.

use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    pub fn on_key(&mut self, key: KeyEvent) {
        // A keystroke ends any in-progress card drag.
        self.drag_card = None;
        self.drag_over = None;
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

    /// The card grid. Arrows and `hjkl` move between cards; `Alt` + those (or
    /// `K` / `J`) reorder the highlighted card among its siblings; `Enter`
    /// descends into a folder card and `Backspace` steps back out. Everything
    /// else falls through to the binder so `n`, `r`, `s`, `i`, `d` still work.
    fn card_key(&mut self, key: KeyEvent) {
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        // Reorder the highlighted card.
        if alt && let Some(sel) = self.selected_id() {
            let delta = match key.code {
                KeyCode::Up | KeyCode::Left | KeyCode::Char('k') | KeyCode::Char('h') => -1,
                KeyCode::Down | KeyCode::Right | KeyCode::Char('j') | KeyCode::Char('l') => 1,
                _ => 0,
            };
            if delta != 0 {
                if self.project.move_vertical(&sel, delta) {
                    self.select_id(&sel);
                    self.dirty = true;
                }
                return;
            }
        }

        if key.code == KeyCode::Backspace {
            self.cards_ascend();
            return;
        }

        let cards = self.cards();
        if cards.is_empty() {
            self.binder_key(key);
            return;
        }

        if key.code == KeyCode::Enter {
            let is_folder = self
                .selected_id()
                .and_then(|s| self.project.nodes.get(&s))
                .map(|n| n.kind == Kind::Folder)
                .unwrap_or(false);
            if is_folder {
                self.cards_descend();
                return;
            }
            // A text card falls through to the binder, which opens the editor.
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
            KeyCode::Char('g') if ctrl => {
                if self.focus == Focus::Editor {
                    self.open_spell();
                } else {
                    self.toggle_spell();
                }
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
            // Some terminals bind F1 to their own help, so `?` opens it too —
            // but not while writing, where `?` is a question mark.
            KeyCode::Char('?') if self.focus != Focus::Editor => {
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
                self.enter_cards();
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
            KeyCode::F(8) => {
                self.do_compile_book();
                true
            }
            KeyCode::F(7) => {
                self.mouse = !self.mouse;
                self.status = if self.mouse {
                    "Mouse on — click, and drag to select text or reorder cards".into()
                } else {
                    "Mouse off — the terminal handles selection and copy again".into()
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
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // Ctrl-B here (not the editor's bold) opens the book settings.
        if ctrl && key.code == KeyCode::Char('b') {
            self.open_book_settings();
            return;
        }

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
            KeyCode::Char('N') => self.open_notes(),
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
            KeyCode::Char('h') => {
                let v = self.project.nodes.get(&id).map(|n| n.heading.clone()).unwrap_or_default();
                self.begin(Prompt::ChapterHeading, &v);
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

        // Formatting: wrap the selection (or the word under the cursor) in
        // Markdown that the editor then styles in place. All on Ctrl, which is
        // the only modifier a terminal reports reliably without configuration
        // — macOS sends Option/Alt as a composed character by default.
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
            // Ctrl-L: centre the line (or every line the selection touches).
            // Ctrl-P: page break — both shadow redundant editor bindings that
            // the arrow keys still cover.
            KeyCode::Char('l') if ctrl => {
                self.toggle_centered();
                return;
            }
            KeyCode::Char('p') if ctrl => {
                self.insert_page_break();
                return;
            }
            // Ctrl-N here adds (or re-edits) an inline comment rather than
            // moving the cursor down — the arrow key still does that.
            KeyCode::Char('n') if ctrl => {
                self.begin_comment();
                return;
            }
            // Ctrl-C copies the selection to the system clipboard — the OS one,
            // not just the editor's internal yank.
            KeyCode::Char('c') if ctrl => {
                self.copy_selection();
                return;
            }
            // The editor's own undo is Ctrl-U; Ctrl-Z is the reflex, so honour
            // it too. Redo stays on Ctrl-R.
            KeyCode::Char('z') if ctrl => {
                if let Some(id) = self.editor_doc() {
                    self.ensure_editor(&id);
                    if let Some(ta) = self.editors.get_mut(&id)
                        && ta.undo()
                    {
                        self.dirty = true;
                    }
                    self.restyle(&id);
                }
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

        // Smart em-dash: a hyphen typed right after another becomes "—".
        if key.code == KeyCode::Char('-')
            && !ctrl
            && !key.modifiers.contains(KeyModifiers::ALT)
            && let Some(ta) = self.editors.get_mut(&id)
        {
            let (row, col) = ta.cursor();
            let chars: Vec<char> = ta.lines()[row].chars().collect();
            let prev = col.checked_sub(1).and_then(|i| chars.get(i));
            let prev2 = col.checked_sub(2).and_then(|i| chars.get(i));
            if prev == Some(&'-') && prev2 != Some(&'-') {
                ta.delete_char();
                ta.insert_char('—');
                self.dirty = true;
                self.restyle(&id);
                return;
            }
        }

        if let Some(ta) = self.editors.get_mut(&id)
            && ta.input(key) {
                self.dirty = true;
            }
    }

    pub(super) fn begin(&mut self, prompt: Prompt, initial: &str) {
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
            Modal::BookSettings => self.book_settings_key(key),
            Modal::Notes => match key.code {
                KeyCode::Esc => {
                    self.modal = Modal::None;
                    self.notes_target = None;
                }
                KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.save_notes();
                }
                _ => {
                    self.notes_input.input(key);
                }
            },
            Modal::Spell => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Up | KeyCode::Char('k') => {
                    self.spell_sel = self.spell_sel.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.spell_sel =
                        (self.spell_sel + 1).min(self.spell_suggestions.len().saturating_sub(1));
                }
                KeyCode::Char('a') => self.spell_learn(),
                KeyCode::Char(c @ '1'..='9') => {
                    let i = c as usize - '1' as usize;
                    if let Some(w) = self.spell_suggestions.get(i).cloned() {
                        self.spell_apply(&w);
                    }
                }
                KeyCode::Enter => {
                    if let Some(w) = self.spell_suggestions.get(self.spell_sel).cloned() {
                        self.spell_apply(&w);
                    }
                }
                _ => {}
            },
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
                // The new node lands just below the row the eye is on: the
                // first child of an expanded folder, or the next sibling
                // otherwise. A new *folder* will not nest into a folder that
                // already holds documents — that folder is a chapter, so you
                // want another chapter beside it, not a sub-folder within.
                let (parent, index) = match self.selected_id() {
                    Some(sel) => {
                        let n = self.project.nodes.get(&sel);
                        let is_open_folder =
                            n.map(|n| n.kind == Kind::Folder && !n.collapsed).unwrap_or(false);
                        let holds_documents = self
                            .project
                            .children
                            .get(&sel)
                            .map(|kids| {
                                kids.iter().any(|k| {
                                    self.project
                                        .nodes
                                        .get(k)
                                        .map(|n| n.kind == Kind::Text)
                                        .unwrap_or(false)
                                })
                            })
                            .unwrap_or(false);
                        let nest = is_open_folder
                            && !(prompt == Prompt::NewFolder && holds_documents);
                        if nest {
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
            Prompt::ChapterHeading => {
                if let Some(id) = self.selected_id() {
                    if let Some(n) = self.project.nodes.get_mut(&id) {
                        n.heading = normalise_heading(&text);
                    }
                    self.dirty = true;
                }
            }
            Prompt::Book(field) => self.commit_book_field(field, text),
            Prompt::Comment => self.apply_comment(text),
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
}

/// Fold what the writer typed into the canonical stored form: nothing for the
/// numbered default, `title` for "use the folder's own title", the text itself
/// otherwise.
fn normalise_heading(text: &str) -> String {
    match text.trim().to_lowercase().as_str() {
        "" | "numbered" | "number" => String::new(),
        "title" | "titled" => "title".to_string(),
        _ => text.trim().to_string(),
    }
}
