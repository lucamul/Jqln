//! View switching, snapshots, search and compile — the actions a keypress or
//! click ultimately fires, kept apart from the dispatch that routes to them.

use super::*;

impl App {
    pub(super) fn open_snapshots(&mut self) {
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

    pub(super) fn run_search(&mut self, query: String) {
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
    pub(super) fn goto_hit(&mut self) {
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

    pub(super) fn compile_subtree(&mut self, id: &str) {
        self.flush();
        let opts = crate::compile::Options::from(&self.project.compile);
        match crate::compile::compile_to_file(&mut self.project, Some(id), &opts) {
            Ok(p) => {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("output").to_string();
                self.status = format!("Compiled selection to {name}");
            }
            Err(e) => self.status = format!("Compile failed: {e}"),
        }
    }

    pub(super) fn do_compile(&mut self) {
        self.flush();
        let opts = crate::compile::Options::from(&self.project.compile);
        match crate::compile::compile_to_file(&mut self.project, None, &opts) {
            Ok(p) => {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("output").to_string();
                self.status = format!("Compiled to {name}");
            }
            Err(e) => self.status = format!("Compile failed: {e}"),
        }
    }

    /// Compile the novel template: a Typst document, and a PDF when `typst` is
    /// installed.
    pub(super) fn do_compile_book(&mut self) {
        self.flush();
        let name = |p: &std::path::Path| {
            p.file_name().and_then(|s| s.to_str()).unwrap_or("output").to_string()
        };
        match crate::book::compile_to_file(&mut self.project) {
            Ok(crate::book::Outcome::Pdf { pdf, typ }) => {
                self.status = format!("Compiled {} (and {})", name(&pdf), name(&typ));
            }
            Ok(crate::book::Outcome::TypstMissing { typ }) => {
                self.status = format!("Wrote {} — install typst for the PDF", name(&typ));
            }
            Ok(crate::book::Outcome::TypstFailed { message, typ }) => {
                self.status = format!("{}: typst — {message}", name(&typ));
            }
            Err(e) => self.status = format!("Book compile failed: {e}"),
        }
    }
}
