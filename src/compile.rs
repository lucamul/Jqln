//! Assembling a subtree back into one continuous manuscript.
//!
//! Compiling is deliberately dumb: walk the tree in order, skip anything the
//! writer marked excluded, and join what remains. Excluding a folder excludes
//! everything beneath it, so a "Research" or "Cut scenes" folder disappears
//! from the output without having to touch each document inside it.

use crate::project::{Kind, NodeId, Project};

pub struct Options {
    /// Emit folder titles as Markdown headings.
    pub folder_headings: bool,
    /// Emit document titles as headings too. Off by default: scene titles are
    /// usually scaffolding for the writer, not part of the finished text.
    pub document_headings: bool,
    pub separator: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            folder_headings: true,
            document_headings: false,
            separator: "\n\n".to_string(),
        }
    }
}

impl From<&crate::project::Compile> for Options {
    fn from(c: &crate::project::Compile) -> Self {
        Options {
            folder_headings: c.folder_headings,
            document_headings: c.document_headings,
            separator: c.separator.clone(),
        }
    }
}

/// Compile the subtree rooted at `root`, or the whole project when `root` is
/// `None`. Returns the assembled Markdown.
pub fn compile(project: &mut Project, root: Option<&str>, opts: &Options) -> String {
    let mut chunks: Vec<String> = Vec::new();
    match root {
        Some(id) => emit(project, id, 1, opts, &mut chunks),
        None => {
            let roots = project.children.get("").cloned().unwrap_or_default();
            for r in roots {
                emit(project, &r, 1, opts, &mut chunks);
            }
        }
    }
    let mut out = chunks.join(&opts.separator);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn emit(project: &mut Project, id: &str, depth: usize, opts: &Options, out: &mut Vec<String>) {
    let Some(node) = project.nodes.get(id).cloned() else {
        return;
    };
    if !node.include {
        return; // Excluding a node excludes its whole subtree.
    }

    let level = depth.min(6);
    match node.kind {
        Kind::Folder => {
            if opts.folder_headings && !node.title.is_empty() {
                out.push(format!("{} {}", "#".repeat(level), node.title));
            }
        }
        Kind::Text => {
            if opts.document_headings && !node.title.is_empty() {
                out.push(format!("{} {}", "#".repeat(level), node.title));
            }
            let body = project.body(id);
            let body = crate::markup::without_comments(&body);
            let body = body.trim_matches('\n');
            if !body.is_empty() {
                out.push(body.to_string());
            }
        }
    }

    let kids: Vec<NodeId> = project.children.get(id).cloned().unwrap_or_default();
    for k in kids {
        emit(project, &k, depth + 1, opts, out);
    }
}

/// Compile and write next to the project directory. Returns the path written.
pub fn compile_to_file(
    project: &mut Project,
    root: Option<&str>,
    opts: &Options,
) -> std::io::Result<std::path::PathBuf> {
    let text = compile(project, root, opts);
    // A subtree compiles under its own title so that compiling a chapter does
    // not overwrite the whole manuscript.
    let stem = match root.and_then(|id| project.nodes.get(id)) {
        Some(node) => crate::project::slugify(&node.title),
        None => crate::project::slugify(&project.meta.name),
    };
    let path = project.root.join(format!("{stem}.md"));
    std::fs::write(&path, text)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Kind, Project, ROOT};

    fn scratch(tag: &str) -> Project {
        let dir = std::env::temp_dir().join(format!("jqln-compile-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = Project::create(&dir, "Book").unwrap();
        // Start from an empty tree so each test builds exactly what it needs.
        let roots: Vec<String> = p.children[ROOT].clone();
        for r in roots {
            p.remove(&r);
        }
        p
    }

    #[test]
    fn joins_documents_in_order_under_folder_headings() {
        let mut p = scratch("order");
        let part = p.insert(ROOT, None, "Part One", Kind::Folder);
        let a = p.insert(&part, None, "Scene A", Kind::Text);
        let b = p.insert(&part, None, "Scene B", Kind::Text);
        p.set_body(&a, "First light.".into());
        p.set_body(&b, "Then dark.".into());

        let out = compile(&mut p, None, &Options::default());
        assert_eq!(out, "# Part One\n\nFirst light.\n\nThen dark.\n");
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn inline_comments_never_reach_the_output() {
        let mut p = scratch("comments");
        let a = p.insert(ROOT, None, "Scene", Kind::Text);
        p.set_body(
            &a,
            "She left{>>too abrupt?<<} early. The {==red==}{>>cut<<} door.".into(),
        );
        let out = compile(&mut p, None, &Options::default());
        assert_eq!(out, "She left early. The red door.\n");
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn a_trashed_chapter_is_not_compiled() {
        let mut p = scratch("trash-compile");
        let keep = p.insert(ROOT, None, "Keep", Kind::Text);
        p.set_body(&keep, "Kept prose.".into());
        let drop = p.insert(ROOT, None, "Drop", Kind::Text);
        p.set_body(&drop, "Trashed prose.".into());
        p.trash(&drop);

        let out = compile(&mut p, None, &Options::default());
        assert!(out.contains("Kept prose."));
        assert!(!out.contains("Trashed prose."), "the Trash is not part of the book");
        assert!(!out.contains("Trash"));
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn excluding_a_folder_drops_its_whole_subtree() {
        let mut p = scratch("exclude");
        let ms = p.insert(ROOT, None, "Manuscript", Kind::Folder);
        let keep = p.insert(&ms, None, "Kept", Kind::Text);
        p.set_body(&keep, "Kept prose.".into());

        let research = p.insert(ROOT, None, "Research", Kind::Folder);
        let note = p.insert(&research, None, "Note", Kind::Text);
        p.set_body(&note, "Should not appear.".into());
        p.nodes.get_mut(&research).unwrap().include = false;

        let out = compile(&mut p, None, &Options::default());
        assert!(out.contains("Kept prose."));
        assert!(!out.contains("Should not appear."), "excluded subtree leaked");
        assert!(!out.contains("Research"));
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn heading_level_follows_depth_and_can_include_documents() {
        let mut p = scratch("depth");
        let part = p.insert(ROOT, None, "Part", Kind::Folder);
        let ch = p.insert(&part, None, "Chapter", Kind::Folder);
        let sc = p.insert(&ch, None, "Scene", Kind::Text);
        p.set_body(&sc, "Body.".into());

        let opts = Options { document_headings: true, ..Default::default() };
        let out = compile(&mut p, None, &opts);
        assert!(out.contains("# Part"));
        assert!(out.contains("## Chapter"));
        assert!(out.contains("### Scene"));
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn compiles_only_the_requested_subtree() {
        let mut p = scratch("subtree");
        let one = p.insert(ROOT, None, "One", Kind::Folder);
        let a = p.insert(&one, None, "A", Kind::Text);
        p.set_body(&a, "Alpha.".into());
        let two = p.insert(ROOT, None, "Two", Kind::Folder);
        let b = p.insert(&two, None, "B", Kind::Text);
        p.set_body(&b, "Beta.".into());

        let out = compile(&mut p, Some(&two), &Options::default());
        assert!(out.contains("Beta."));
        assert!(!out.contains("Alpha."));
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn empty_documents_do_not_leave_blank_gaps() {
        let mut p = scratch("empty");
        let f = p.insert(ROOT, None, "F", Kind::Folder);
        let a = p.insert(&f, None, "A", Kind::Text);
        let _blank = p.insert(&f, None, "Blank", Kind::Text);
        let c = p.insert(&f, None, "C", Kind::Text);
        p.set_body(&a, "One.".into());
        p.set_body(&c, "Two.".into());

        let out = compile(&mut p, None, &Options::default());
        assert_eq!(out, "# F\n\nOne.\n\nTwo.\n");
        assert!(!out.contains("\n\n\n"), "empty document left a triple newline");
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn formatting_markup_passes_straight_through() {
        let mut p = scratch("markup");
        let f = p.insert(ROOT, None, "F", Kind::Folder);
        let a = p.insert(&f, None, "A", Kind::Text);
        p.set_body(
            &a,
            "A **bold** word.\n\n\\newpage\n\n::: center\nfin\n:::".into(),
        );
        let out = compile(&mut p, None, &Options::default());
        assert!(out.contains("**bold**"), "inline markup must survive compile");
        assert!(out.contains("\\newpage"), "page breaks must survive compile");
        assert!(out.contains("::: center"), "centre fences must survive compile");
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn writes_a_file_next_to_the_project() {
        let mut p = scratch("file");
        let f = p.insert(ROOT, None, "F", Kind::Folder);
        let a = p.insert(&f, None, "A", Kind::Text);
        p.set_body(&a, "Text.".into());

        let path = compile_to_file(&mut p, None, &Options::default()).unwrap();
        assert!(path.ends_with("book.md"));
        assert!(std::fs::read_to_string(&path).unwrap().contains("Text."));
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn options_follow_the_projects_compile_settings() {
        let mut p = scratch("opts");
        let f = p.insert(ROOT, None, "Part", Kind::Folder);
        let a = p.insert(&f, None, "Scene", Kind::Text);
        p.set_body(&a, "Body.".into());

        p.compile.folder_headings = false;
        p.compile.document_headings = true;
        p.compile.separator = "\n\n* * *\n\n".into();

        let opts = Options::from(&p.compile);
        let out = compile(&mut p, None, &opts);
        assert!(!out.contains("# Part"), "folder headings suppressed");
        assert!(out.contains("# Scene"), "document headings emitted");
        let _ = std::fs::remove_dir_all(&p.root);
    }
}
