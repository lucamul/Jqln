//! In-place formatting: bold, italic, centred blocks and page breaks, plus the
//! live re-styling that fades the Markdown markers as you write.

use super::*;
use tui_textarea::CursorMove;

impl App {
    /// Is text selected in the document currently being edited?
    pub(super) fn has_selection(&self) -> bool {
        self.editor_doc()
            .and_then(|id| self.editors.get(&id))
            .and_then(|ta| ta.selection_range())
            .is_some()
    }

    /// Recompute the in-place formatting highlights for one editor. Cheap
    /// enough to run on every frame; the editor keeps no styling of its own.
    pub fn restyle(&mut self, id: &str) {
        let spell_marks = self.spell_marks(id);
        if let Some(ta) = self.editors.get_mut(id) {
            let marks = crate::markup::highlights(ta.lines());
            ta.clear_custom_highlight();
            for (range, style, priority) in marks.into_iter().chain(spell_marks) {
                ta.custom_highlight(range, style, priority);
            }
        }
    }

    /// Misspelling highlights for one editor, recomputed only when its text has
    /// changed since the last call — `restyle` runs every frame, and in
    /// continuous mode over every document on screen.
    fn spell_marks(&mut self, id: &str) -> Vec<crate::markup::Highlight> {
        use std::hash::{Hash, Hasher};
        if !self.spell_on {
            return Vec::new();
        }
        let Some(ta) = self.editors.get(id) else {
            return Vec::new();
        };
        let lines = ta.lines();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        lines.hash(&mut hasher);
        let hash = hasher.finish();
        if let Some((cached, marks)) = self.spell_cache.get(id)
            && *cached == hash
        {
            return marks.clone();
        }
        let marks = self.spell.highlights(lines);
        self.spell_cache.insert(id.to_string(), (hash, marks.clone()));
        marks
    }

    /// Flip spell checking on or off, telling the writer and remembering the
    /// choice in `jqln.toml`.
    pub(super) fn toggle_spell(&mut self) {
        self.spell_on = !self.spell_on;
        self.project.spelling.enabled = self.spell_on;
        self.dirty = true;
        self.status = if self.spell_on {
            "Spell check on".into()
        } else {
            "Spell check off".into()
        };
        if let Some(id) = self.editor_doc() {
            self.restyle(&id);
        }
    }

    /// Ctrl-G in the editor: open the corrections list for the misspelled word
    /// under the cursor, or say why there is nothing to correct.
    pub(super) fn open_spell(&mut self) {
        if !self.spell_on {
            self.toggle_spell();
            return;
        }
        let Some(id) = self.editor_doc() else { return };
        self.ensure_editor(&id);
        let Some(ta) = self.editors.get(&id) else { return };
        let (row, col) = ta.cursor();
        let line = ta.lines()[row].clone();
        let Some((start, end, word)) = crate::spell::word_at(&line, col) else {
            self.status = "No word under the cursor".into();
            return;
        };
        if self.spell.is_correct(&word) {
            self.status = format!("“{word}” is spelled correctly");
            return;
        }
        self.spell_word = word.clone();
        self.spell_at = (row, start, end);
        self.spell_suggestions = self.spell.suggestions(&word);
        self.spell_sel = 0;
        self.modal = Modal::Spell;
    }

    /// Replace the flagged word with `replacement` and close the modal.
    pub(super) fn spell_apply(&mut self, replacement: &str) {
        let Some(id) = self.editor_doc() else {
            self.modal = Modal::None;
            return;
        };
        let (row, start, end) = self.spell_at;
        if let Some(ta) = self.editors.get_mut(&id) {
            ta.move_cursor(CursorMove::Jump(row as u16, start as u16));
            ta.start_selection();
            ta.move_cursor(CursorMove::Jump(row as u16, end as u16));
            ta.insert_str(replacement);
            self.dirty = true;
        }
        self.restyle(&id);
        self.modal = Modal::None;
    }

    /// Add the flagged word to the project's personal list so it stops being
    /// underlined, here and on every later open.
    pub(super) fn spell_learn(&mut self) {
        let word = self.spell_word.clone();
        if !word.is_empty() && !self.project.spelling.words.contains(&word) {
            self.spell.learn(&word);
            self.project.spelling.words.push(word.clone());
            self.spell_cache.clear();
            self.dirty = true;
        }
        self.status = format!("Added “{word}” to your dictionary");
        if let Some(id) = self.editor_doc() {
            self.restyle(&id);
        }
        self.modal = Modal::None;
    }

    /// Open the notes modal for the selected node (document or folder).
    pub(super) fn open_notes(&mut self) {
        let Some(id) = self.selected_id() else { return };
        let existing = self.project.note(&id);
        self.notes_input = multiline(&existing);
        self.notes_target = Some(id);
        self.modal = Modal::Notes;
    }

    /// Persist the notes buffer to the node it was opened for.
    pub(super) fn save_notes(&mut self) {
        if let Some(id) = self.notes_target.take() {
            let text = self.notes_input.lines().join("\n");
            self.project.set_note(&id, text);
            self.dirty = true;
            self.status = if self.project.has_note(&id) {
                "Note saved".into()
            } else {
                "Note cleared".into()
            };
        }
        self.modal = Modal::None;
    }

    /// Ctrl-N in the editor: prompt for a comment. On an existing comment this
    /// re-edits it; otherwise it is added at the cursor or around the selection.
    pub(super) fn begin_comment(&mut self) {
        let Some(id) = self.editor_doc() else { return };
        self.ensure_editor(&id);
        let Some(ta) = self.editors.get(&id) else { return };
        let (row, col) = ta.cursor();
        let line = ta.lines()[row].clone();
        match crate::markup::comment_at(&line, col) {
            Some(hit) => {
                let text = hit.text.clone();
                self.comment_edit = Some((row, hit));
                self.begin(Prompt::Comment, &text);
            }
            None => {
                self.comment_edit = None;
                self.begin(Prompt::Comment, "");
            }
        }
    }

    /// Commit the comment text: replace an existing marker (or delete it when
    /// the text is cleared), or insert a new one.
    pub(super) fn apply_comment(&mut self, text: String) {
        let Some(id) = self.editor_doc() else {
            self.comment_edit = None;
            return;
        };
        self.ensure_editor(&id);
        let Some(ta) = self.editors.get_mut(&id) else { return };

        if let Some((row, hit)) = self.comment_edit.take() {
            let line = ta.lines()[row].clone();
            // Clearing a comment drops the marker but keeps the text it flagged.
            let (from, to, replacement) = if text.is_empty() {
                (hit.full.0, hit.full.1, hit.flagged.unwrap_or_default())
            } else {
                (hit.comment.0, hit.comment.1, format!("{{>>{text}<<}}"))
            };
            let c0 = crate::markup::char_index(&line, from) as u16;
            let c1 = crate::markup::char_index(&line, to) as u16;
            ta.move_cursor(CursorMove::Jump(row as u16, c0));
            ta.start_selection();
            ta.move_cursor(CursorMove::Jump(row as u16, c1));
            ta.insert_str(&replacement);
            self.dirty = true;
            self.restyle(&id);
            return;
        }

        if text.is_empty() {
            return;
        }

        match ta.selection_range() {
            Some(((sr, sc), (er, ec))) if sr == er && sc != ec => {
                let line = ta.lines()[sr].clone();
                let sel = &line[crate::markup::byte_index(&line, sc)
                    ..crate::markup::byte_index(&line, ec)];
                let wrapped = format!("{{=={sel}==}}{{>>{text}<<}}");
                ta.move_cursor(CursorMove::Jump(sr as u16, sc as u16));
                ta.start_selection();
                ta.move_cursor(CursorMove::Jump(er as u16, ec as u16));
                ta.insert_str(wrapped);
            }
            _ => {
                ta.cancel_selection();
                ta.insert_str(format!("{{>>{text}<<}}"));
            }
        }
        self.dirty = true;
        self.restyle(&id);
    }

    /// Toggle a `marker` (`*` or `**`) around the selection, or the word under
    /// the cursor when nothing is selected. The affected text stays selected
    /// so the same key toggles it straight back off.
    pub(super) fn format_inline(&mut self, marker: &'static str) {
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

    /// Wrap in a `::: center` / `:::` fence every line the selection touches —
    /// or just the current line when nothing is selected — and peel the fence
    /// back off when it is already there.
    pub(super) fn toggle_centered(&mut self) {
        let Some(id) = self.editor_doc() else { return };
        self.ensure_editor(&id);
        let Some(ta) = self.editors.get_mut(&id) else { return };

        let (cur_row, cur_col) = ta.cursor();
        let (first, last) = match ta.selection_range() {
            // A selection that ends at column 0 does not really reach into
            // that row, so do not fence it.
            Some(((sr, _), (er, ec))) if er > sr && ec == 0 => (sr, er - 1),
            Some(((sr, _), (er, _))) => (sr, er),
            None => (cur_row, cur_row),
        };

        let mut lines: Vec<String> = ta.lines().to_vec();
        let open = crate::markup::CENTER_OPEN;
        let close = crate::markup::CENTER_CLOSE;

        // Already centred? The enclosing fence may be right against the range
        // or several lines out, so search outward, stopping at the next block's
        // boundary so two adjacent blocks are not merged.
        let open_row = (0..first)
            .rev()
            .take_while(|&r| lines[r].trim() != close)
            .find(|&r| lines[r].trim() == open);
        let close_row = (last + 1..lines.len())
            .take_while(|&r| lines[r].trim() != open)
            .find(|&r| lines[r].trim() == close);

        let row_shift: isize = match (open_row, close_row) {
            (Some(o), Some(c)) => {
                lines.remove(c);
                lines.remove(o);
                -1
            }
            _ => {
                lines.insert(last + 1, close.to_string());
                lines.insert(first, open.to_string());
                1
            }
        };

        ta.select_all();
        ta.insert_str(lines.join("\n"));
        let row = (cur_row as isize + row_shift).max(0) as u16;
        ta.move_cursor(CursorMove::Jump(row, cur_col as u16));

        self.dirty = true;
        self.restyle(&id);
    }

    /// Drop a `\newpage` marker on its own line after the current one.
    pub(super) fn insert_page_break(&mut self) {
        let Some(id) = self.editor_doc() else { return };
        self.ensure_editor(&id);
        let Some(ta) = self.editors.get_mut(&id) else { return };
        ta.move_cursor(CursorMove::End);
        ta.insert_str(format!("\n\n{}\n", crate::markup::PAGE_BREAK));
        self.dirty = true;
        self.restyle(&id);
    }
}
