//! Project model and on-disk persistence.
//!
//! A project is a directory:
//!
//! ```text
//! MyNovel/
//!   jqln.toml        manifest: tree structure + all metadata
//!   docs/
//!     a1b2c3-opening-scene.md
//!     ...
//! ```
//!
//! The manifest is the source of truth for tree shape and ordering; the `.md`
//! files hold nothing but prose so they stay greppable and externally editable.

mod search;
mod snapshot;
mod text;

pub use search::Hit;
pub use text::{count_words, now_year, pretty_stamp, slugify};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub type NodeId = String;

/// Key used in `children` for top-level nodes.
pub const ROOT: &str = "";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Folder,
    Text,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub title: String,
    pub kind: Kind,
    /// Empty string means top level.
    #[serde(default)]
    pub parent: NodeId,
    /// Filename relative to `docs/`. Empty for folders.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub synopsis: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Whether this node is emitted by `compile`.
    #[serde(default = "default_true")]
    pub include: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
}

impl Node {
    pub fn new(id: NodeId, title: impl Into<String>, kind: Kind) -> Self {
        let title = title.into();
        let file = match kind {
            Kind::Text => format!("{}-{}.md", id, slugify(&title)),
            Kind::Folder => String::new(),
        };
        Node {
            id,
            title,
            kind,
            parent: ROOT.to_string(),
            file,
            synopsis: String::new(),
            label: String::new(),
            status: String::new(),
            keywords: Vec::new(),
            include: true,
            collapsed: false,
        }
    }

}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub name: String,
    /// Omitted from the manifest while it holds the default, so the common
    /// case stays free of an awkward multi-line TOML string.
    #[serde(default = "default_sep", skip_serializing_if = "is_default_sep")]
    pub compile_separator: String,
}

fn default_sep() -> String {
    "\n\n".to_string()
}

fn is_default_sep(s: &String) -> bool {
    s == "\n\n"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Targets {
    #[serde(default)]
    pub project_words: usize,
    #[serde(default)]
    pub session_words: usize,
}

impl Default for Targets {
    fn default() -> Self {
        Targets {
            project_words: 50_000,
            session_words: 500,
        }
    }
}

/// Everything the PDF ("book") compile needs that is not in the prose itself:
/// the title-page details, the trim size, the type. Written into every
/// manifest so the knobs are discoverable — edit them in `jqln.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Book {
    /// Printed title. Empty falls back to the project name.
    pub title: String,
    pub subtitle: String,
    pub author: String,
    /// 0 means "the year the book is compiled".
    pub copyright_year: u32,
    /// Empty falls back to the author.
    pub copyright_holder: String,
    pub publisher: String,
    pub rights: String,
    /// A short dedication. When set and no dedication document exists, a
    /// dedication page is generated for it.
    pub dedication: String,
    /// Trim size: one of `5x8`, `5.25x8`, `5.5x8.5`, `6x9`, `a5`.
    pub trim: String,
    pub body_font: String,
    pub body_size: f32,
    /// Word before the chapter number, e.g. `Chapter`. Empty for a bare numeral.
    pub chapter_label: String,
    /// Glyphs drawn between scenes within a chapter.
    pub scene_break: String,
    /// Running headers (author verso, title recto) through the body.
    pub running_heads: bool,
    /// Start every chapter on a right-hand page. Matches print convention, at
    /// the cost of the occasional blank verso.
    pub chapters_on_recto: bool,
    /// Binder folder holding the front matter. Empty means "Front Matter".
    pub front_matter_folder: String,
}

impl Default for Book {
    fn default() -> Self {
        Book {
            title: String::new(),
            subtitle: String::new(),
            author: String::new(),
            copyright_year: 0,
            copyright_holder: String::new(),
            publisher: String::new(),
            rights: "All rights reserved.".to_string(),
            dedication: String::new(),
            trim: "5.5x8.5".to_string(),
            body_font: "Libertinus Serif".to_string(),
            body_size: 11.0,
            chapter_label: "Chapter".to_string(),
            scene_break: "•   •   •".to_string(),
            running_heads: true,
            chapters_on_recto: false,
            front_matter_folder: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    project: Meta,
    #[serde(default)]
    targets: Targets,
    #[serde(default)]
    book: Book,
    #[serde(default, rename = "node")]
    nodes: Vec<Node>,
}

/// In-memory project. `children` is derived from `Node::parent` on load and is
/// the authoritative ordering while the app runs; it is flattened back into a
/// pre-order `[[node]]` list on save.
pub struct Project {
    pub root: PathBuf,
    pub meta: Meta,
    pub targets: Targets,
    pub book: Book,
    pub nodes: HashMap<NodeId, Node>,
    pub children: HashMap<NodeId, Vec<NodeId>>,
    /// Document bodies, lazily loaded from disk.
    pub bodies: HashMap<NodeId, String>,
    next_seq: u64,
}

impl Project {
    pub fn docs_dir(&self) -> PathBuf {
        self.root.join("docs")
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("jqln.toml")
    }

    /// Create a new project on disk with a starter structure.
    pub fn create(root: &Path, name: &str) -> std::io::Result<Project> {
        std::fs::create_dir_all(root.join("docs"))?;
        let mut p = Project {
            root: root.to_path_buf(),
            meta: Meta {
                name: name.to_string(),
                compile_separator: default_sep(),
            },
            targets: Targets::default(),
            book: Book::default(),
            nodes: HashMap::new(),
            children: HashMap::new(),
            bodies: HashMap::new(),
            next_seq: 0,
        };
        p.children.insert(ROOT.to_string(), Vec::new());

        let manuscript = p.insert(ROOT, None, "Manuscript", Kind::Folder);
        let ch = p.insert(&manuscript, None, "Chapter One", Kind::Folder);
        let scene = p.insert(&ch, None, "Opening Scene", Kind::Text);
        p.bodies.insert(scene.clone(), String::new());
        if let Some(n) = p.nodes.get_mut(&scene) {
            n.synopsis = "The one where it begins.".to_string();
        }
        let research = p.insert(ROOT, None, "Research", Kind::Folder);
        if let Some(n) = p.nodes.get_mut(&research) {
            n.include = false;
        }

        p.save()?;
        Ok(p)
    }

    pub fn open(root: &Path) -> std::io::Result<Project> {
        let text = std::fs::read_to_string(root.join("jqln.toml"))?;
        let manifest: Manifest = toml::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        let mut nodes = HashMap::new();
        let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        children.insert(ROOT.to_string(), Vec::new());

        // File order defines sibling order.
        for n in manifest.nodes {
            children.entry(n.parent.clone()).or_default().push(n.id.clone());
            children.entry(n.id.clone()).or_default();
            nodes.insert(n.id.clone(), n);
        }
        // Drop references to parents that do not exist (corrupt manifest safety).
        let known: Vec<NodeId> = nodes.keys().cloned().collect();
        for (parent, kids) in children.iter_mut() {
            if !parent.is_empty() && !nodes.contains_key(parent) {
                kids.clear();
            }
        }
        let _ = known;

        Ok(Project {
            root: root.to_path_buf(),
            meta: manifest.project,
            targets: manifest.targets,
            book: manifest.book,
            nodes,
            children,
            bodies: HashMap::new(),
            next_seq: 0,
        })
    }

    fn gen_id(&mut self) -> NodeId {
        self.next_seq += 1;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let mut v = nanos.wrapping_mul(31).wrapping_add(self.next_seq);
        let mut s = String::new();
        const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
        for _ in 0..7 {
            s.push(ALPHABET[(v % 36) as usize] as char);
            v /= 36;
        }
        if self.nodes.contains_key(&s) {
            return self.gen_id();
        }
        s
    }

    /// Insert a new node under `parent`, at `index` (or appended when `None`).
    pub fn insert(
        &mut self,
        parent: &str,
        index: Option<usize>,
        title: &str,
        kind: Kind,
    ) -> NodeId {
        let id = self.gen_id();
        let mut node = Node::new(id.clone(), title, kind);
        node.parent = parent.to_string();
        let kids = self.children.entry(parent.to_string()).or_default();
        let at = index.unwrap_or(kids.len()).min(kids.len());
        kids.insert(at, id.clone());
        self.children.entry(id.clone()).or_default();
        self.nodes.insert(id.clone(), node);
        if kind == Kind::Text {
            self.bodies.insert(id.clone(), String::new());
        }
        id
    }

    pub fn parent_of(&self, id: &str) -> NodeId {
        self.nodes
            .get(id)
            .map(|n| n.parent.clone())
            .unwrap_or_else(|| ROOT.to_string())
    }

    pub fn index_in_parent(&self, id: &str) -> usize {
        let p = self.parent_of(id);
        self.children
            .get(&p)
            .and_then(|k| k.iter().position(|c| c == id))
            .unwrap_or(0)
    }

    fn detach(&mut self, id: &str) {
        let p = self.parent_of(id);
        if let Some(kids) = self.children.get_mut(&p) {
            kids.retain(|c| c != id);
        }
    }

    fn attach(&mut self, id: &str, parent: &str, index: usize) {
        let kids = self.children.entry(parent.to_string()).or_default();
        let at = index.min(kids.len());
        kids.insert(at, id.to_string());
        if let Some(n) = self.nodes.get_mut(id) {
            n.parent = parent.to_string();
        }
    }

    /// Move a node among its siblings. Returns true when something moved.
    pub fn move_vertical(&mut self, id: &str, delta: isize) -> bool {
        let p = self.parent_of(id);
        let idx = self.index_in_parent(id) as isize;
        let len = self.children.get(&p).map(|k| k.len()).unwrap_or(0) as isize;
        let new = idx + delta;
        if new < 0 || new >= len {
            return false;
        }
        if let Some(kids) = self.children.get_mut(&p) {
            kids.swap(idx as usize, new as usize);
        }
        true
    }

    /// Indent: become the last child of the preceding sibling.
    pub fn indent(&mut self, id: &str) -> bool {
        let p = self.parent_of(id);
        let idx = self.index_in_parent(id);
        if idx == 0 {
            return false;
        }
        let new_parent = match self.children.get(&p) {
            Some(kids) => kids[idx - 1].clone(),
            None => return false,
        };
        self.detach(id);
        let at = self.children.entry(new_parent.clone()).or_default().len();
        self.attach(id, &new_parent, at);
        if let Some(n) = self.nodes.get_mut(&new_parent) {
            n.collapsed = false;
        }
        true
    }

    /// Outdent: become the next sibling of the current parent.
    pub fn outdent(&mut self, id: &str) -> bool {
        let p = self.parent_of(id);
        if p.is_empty() {
            return false;
        }
        let grandparent = self.parent_of(&p);
        let at = self.index_in_parent(&p) + 1;
        self.detach(id);
        self.attach(id, &grandparent, at);
        true
    }

    /// Remove a node and its whole subtree. Returns removed ids.
    pub fn remove(&mut self, id: &str) -> Vec<NodeId> {
        let mut removed = Vec::new();
        self.collect_subtree(id, &mut removed);
        self.detach(id);
        for r in &removed {
            if let Some(n) = self.nodes.remove(r)
                && !n.file.is_empty() {
                    let _ = std::fs::remove_file(self.root.join("docs").join(&n.file));
                }
            self.children.remove(r);
            self.bodies.remove(r);
        }
        removed
    }

    pub fn collect_subtree(&self, id: &str, out: &mut Vec<NodeId>) {
        out.push(id.to_string());
        if let Some(kids) = self.children.get(id) {
            for k in kids.clone() {
                self.collect_subtree(&k, out);
            }
        }
    }

    /// Pre-order walk of the whole tree, yielding `(id, depth)`.
    pub fn walk(&self) -> Vec<(NodeId, usize)> {
        let mut out = Vec::new();
        self.walk_from(ROOT, 0, false, &mut out);
        out
    }

    /// Pre-order walk that skips the children of collapsed nodes.
    pub fn visible(&self) -> Vec<(NodeId, usize)> {
        let mut out = Vec::new();
        self.walk_from(ROOT, 0, true, &mut out);
        out
    }

    fn walk_from(&self, parent: &str, depth: usize, respect_collapse: bool, out: &mut Vec<(NodeId, usize)>) {
        let Some(kids) = self.children.get(parent) else {
            return;
        };
        for k in kids {
            out.push((k.clone(), depth));
            let collapsed = respect_collapse
                && self.nodes.get(k).map(|n| n.collapsed).unwrap_or(false);
            if !collapsed {
                self.walk_from(k, depth + 1, respect_collapse, out);
            }
        }
    }

    /// Text descendants of `id` in document order, including `id` itself.
    pub fn text_descendants(&self, id: &str) -> Vec<NodeId> {
        let mut all = Vec::new();
        self.collect_subtree(id, &mut all);
        all.into_iter()
            .filter(|i| self.nodes.get(i).map(|n| n.kind == Kind::Text).unwrap_or(false))
            .collect()
    }

    /// Load a document body from disk if not already in memory.
    pub fn body(&mut self, id: &str) -> String {
        if let Some(b) = self.bodies.get(id) {
            return b.clone();
        }
        let file = self.nodes.get(id).map(|n| n.file.clone()).unwrap_or_default();
        let body = if file.is_empty() {
            String::new()
        } else {
            std::fs::read_to_string(self.docs_dir().join(&file)).unwrap_or_default()
        };
        self.bodies.insert(id.to_string(), body.clone());
        body
    }

    pub fn set_body(&mut self, id: &str, text: String) {
        self.bodies.insert(id.to_string(), text);
    }

    /// Word count for a single document, loading it if needed.
    pub fn word_count(&mut self, id: &str) -> usize {
        let b = self.body(id);
        count_words(&b)
    }

    /// Word count across every text document in the project.
    pub fn total_words(&mut self) -> usize {
        let ids: Vec<NodeId> = self
            .walk()
            .into_iter()
            .map(|(i, _)| i)
            .filter(|i| self.nodes.get(i).map(|n| n.kind == Kind::Text).unwrap_or(false))
            .collect();
        ids.iter().map(|i| self.word_count(i)).sum()
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.docs_dir())?;

        // Write bodies for every loaded document.
        let entries: Vec<(NodeId, String)> = self
            .bodies
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (id, body) in entries {
            let Some(node) = self.nodes.get(&id) else { continue };
            if node.file.is_empty() {
                continue;
            }
            let path = self.docs_dir().join(&node.file);
            std::fs::write(path, body)?;
        }

        // Flatten the tree pre-order so the manifest reads top-to-bottom.
        let ordered: Vec<Node> = self
            .walk()
            .into_iter()
            .filter_map(|(id, _)| self.nodes.get(&id).cloned())
            .collect();

        let manifest = Manifest {
            project: self.meta.clone(),
            targets: self.targets.clone(),
            book: self.book.clone(),
            nodes: ordered,
        };
        let text = toml::to_string_pretty(&manifest)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        // Write via temp file so a crash mid-write cannot destroy the manifest.
        let final_path = self.manifest_path();
        let tmp = self.root.join("jqln.toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &final_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("jqln-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn create_open_roundtrip() {
        let dir = scratch("roundtrip");
        let mut p = Project::create(&dir, "Test").unwrap();
        let ids: Vec<String> = p.walk().into_iter().map(|(i, _)| i).collect();
        assert_eq!(ids.len(), 4, "starter tree should have 4 nodes");
        let scene = ids
            .iter()
            .find(|i| p.nodes[*i].title == "Opening Scene")
            .unwrap()
            .clone();
        p.set_body(&scene, "Hello there world.".into());
        p.save().unwrap();

        // The manifest is meant to be read and diffed by a human.
        let raw = std::fs::read_to_string(dir.join("jqln.toml")).unwrap();
        assert!(
            !raw.contains("compile_separator"),
            "a default separator should stay out of the manifest, not appear as a \
             multi-line TOML string"
        );
        assert!(raw.contains("title = \"Opening Scene\""));

        let mut q = Project::open(&dir).unwrap();
        assert_eq!(q.meta.name, "Test");
        let titles: Vec<String> = q.walk().into_iter().map(|(i, _)| q.nodes[&i].title.clone()).collect();
        assert_eq!(titles, ["Manuscript", "Chapter One", "Opening Scene", "Research"]);
        assert_eq!(q.body(&scene), "Hello there world.");
        assert_eq!(q.total_words(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn indent_outdent_and_move() {
        let dir = scratch("tree");
        let mut p = Project::create(&dir, "T").unwrap();
        let a = p.insert(ROOT, None, "A", Kind::Text);
        let b = p.insert(ROOT, None, "B", Kind::Text);

        // B indents under A.
        assert!(p.indent(&b));
        assert_eq!(p.parent_of(&b), a);
        // and back out.
        assert!(p.outdent(&b));
        assert_eq!(p.parent_of(&b), ROOT);
        // First child cannot indent.
        let first = p.children[ROOT].first().unwrap().clone();
        assert!(!p.indent(&first));
        // Move B up past A.
        let before = p.index_in_parent(&b);
        assert!(p.move_vertical(&b, -1));
        assert_eq!(p.index_in_parent(&b), before - 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_takes_the_whole_subtree() {
        let dir = scratch("remove");
        let mut p = Project::create(&dir, "T").unwrap();
        let parent = p.insert(ROOT, None, "P", Kind::Folder);
        let child = p.insert(&parent, None, "C", Kind::Text);
        let removed = p.remove(&parent);
        assert_eq!(removed.len(), 2);
        assert!(!p.nodes.contains_key(&child));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

