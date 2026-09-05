//! Assembling what the model sees: a system prompt, a frame describing the
//! project, and the document text the writer has chosen to share.

use crate::project::{Kind, NodeId, Project};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// Just the open document.
    Document,
    /// The open document plus the project outline (titles + counts, no bodies).
    DocumentOutline,
    /// Every document in the open document's folder.
    Chapter,
    /// Every document in the project, in order.
    Manuscript,
    /// Only the current selection.
    Selection,
}

impl Scope {
    pub fn parse(s: &str) -> Option<Scope> {
        Some(match s.trim().to_lowercase().as_str() {
            "document" | "doc" | "file" => Scope::Document,
            "document+outline" | "doc+outline" | "outline" => Scope::DocumentOutline,
            "chapter" | "folder" => Scope::Chapter,
            "manuscript" | "all" | "project" => Scope::Manuscript,
            "selection" | "sel" => Scope::Selection,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Scope::Document => "document",
            Scope::DocumentOutline => "document + outline",
            Scope::Chapter => "chapter",
            Scope::Manuscript => "manuscript",
            Scope::Selection => "selection",
        }
    }

    /// The canonical config string (round-trips through `parse`).
    pub fn key(self) -> &'static str {
        match self {
            Scope::Document => "document",
            Scope::DocumentOutline => "document+outline",
            Scope::Chapter => "chapter",
            Scope::Manuscript => "manuscript",
            Scope::Selection => "selection",
        }
    }

    pub fn next(self) -> Scope {
        match self {
            Scope::Document => Scope::DocumentOutline,
            Scope::DocumentOutline => Scope::Chapter,
            Scope::Chapter => Scope::Manuscript,
            Scope::Manuscript => Scope::Selection,
            Scope::Selection => Scope::Document,
        }
    }
}

pub const SYSTEM: &str = "\
You are an editor's assistant inside Jqln, a terminal writing studio for \
long-form prose. Help the writer think about structure, pacing, clarity and \
voice. Be concrete and brief. Quote the passage you mean rather than \
describing its location. Do not rewrite the writer's prose unless they \
explicitly ask; prefer pointed questions and specific observations.";

pub const SYSTEM_COMMENTS: &str = "\
\n\nWhen you have remarks anchored to specific passages, you MAY end your reply \
with a single fenced block:\n\
```jqln-comments\n\
[{\"quote\": \"an exact run of text from the document\", \"note\": \"your remark\"}]\n\
```\n\
Each quote must be copied verbatim from the current document and be short \
(a phrase or one sentence). Omit the block if you have no anchored remarks.";

/// The context block and a one-line summary of it for the pane header.
pub struct Built {
    pub text: String,
    pub summary: String,
}

/// Build the context for `scope`. `current` is the open document's id (if any);
/// `selection` is the selected text (if any). Bodies are read from `project`,
/// so the caller should flush editors first.
pub fn build(
    project: &mut Project,
    current: Option<&NodeId>,
    selection: Option<&str>,
    scope: Scope,
) -> Built {
    let mut out = format!("Project: {}\n", project.meta.name);

    if let Some(id) = current
        && let Some(node) = project.nodes.get(id)
    {
        out.push_str(&format!("Open document: {}\n", node.title));
        if !node.synopsis.is_empty() {
            out.push_str(&format!("Synopsis: {}\n", node.synopsis));
        }
    }

    let (docs, summary): (Vec<NodeId>, String) = match scope {
        Scope::Selection => {
            let sel = selection.unwrap_or("").trim();
            out.push_str("\n--- selection ---\n");
            out.push_str(sel);
            out.push('\n');
            let n = crate::project::count_words(sel);
            return Built { text: out, summary: format!("selection ({n} w)") };
        }
        Scope::Document => (current.into_iter().cloned().collect(), String::new()),
        Scope::DocumentOutline => {
            out.push_str("\n--- outline ---\n");
            out.push_str(&outline(project));
            (current.into_iter().cloned().collect(), " + outline".to_string())
        }
        Scope::Chapter => {
            let folder = current.map(|c| {
                if project.nodes.get(c).map(|n| n.kind == Kind::Folder).unwrap_or(false) {
                    c.clone()
                } else {
                    project.parent_of(c)
                }
            });
            let ids = folder.map(|f| project.text_descendants(&f)).unwrap_or_default();
            (ids, String::new())
        }
        Scope::Manuscript => {
            let ids: Vec<NodeId> = project
                .walk()
                .into_iter()
                .map(|(i, _)| i)
                .filter(|i| {
                    project.nodes.get(i).map(|n| n.kind == Kind::Text).unwrap_or(false)
                        && !project.is_trashed(i)
                })
                .collect();
            (ids, String::new())
        }
    };

    let mut words = 0usize;
    for id in &docs {
        let title = project.nodes.get(id).map(|n| n.title.clone()).unwrap_or_default();
        let body = project.body(id);
        words += crate::project::count_words(&body);
        out.push_str(&format!("\n--- {title} ---\n{}\n", body.trim_matches('\n')));
    }

    let label = match (scope, docs.len()) {
        (Scope::Document | Scope::DocumentOutline, _) => {
            let name = current
                .and_then(|c| project.nodes.get(c))
                .map(|n| n.title.clone())
                .unwrap_or_else(|| "nothing".to_string());
            format!("{name}{summary} ({words} w)")
        }
        (Scope::Chapter, n) => format!("chapter · {n} docs ({words} w)"),
        (Scope::Manuscript, n) => format!("manuscript · {n} docs ({words} w)"),
        _ => summary,
    };

    Built { text: out, summary: label }
}

fn outline(project: &mut Project) -> String {
    let mut s = String::new();
    for (id, depth) in project.walk() {
        if project.is_trashed(&id) {
            continue;
        }
        let indent = "  ".repeat(depth);
        let (title, kind, words) = {
            let n = &project.nodes[&id];
            (n.title.clone(), n.kind, n.kind == Kind::Text)
        };
        if words {
            let w = project.word_count(&id);
            s.push_str(&format!("{indent}{title} — {w} w\n"));
        } else {
            let _ = kind;
            s.push_str(&format!("{indent}{title}/\n"));
        }
    }
    s
}
