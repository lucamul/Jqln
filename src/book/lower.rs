//! Lowering: the project's tree and prose turned into Typst source. The parts
//! that are pure functions of the model, kept out of the compile orchestration.

use crate::markup::Segment;
use crate::project::{Book, Kind, Node, NodeId, Project, ROOT};

pub(super) fn front_matter_folder(project: &Project) -> Option<NodeId> {
    let want = {
        let w = project.book.front_matter_folder.trim();
        if w.is_empty() { "front matter".to_string() } else { w.to_lowercase() }
    };
    project
        .children
        .get(ROOT)?
        .iter()
        .find(|id| {
            project
                .nodes
                .get(*id)
                .map(|n| n.kind == Kind::Folder && n.title.trim().to_lowercase() == want)
                .unwrap_or(false)
        })
        .cloned()
}

pub(super) enum Item {
    Part { title: String },
    Chapter { head: ChapterHead, scenes: Vec<NodeId> },
}

/// How a chapter's opening line reads.
pub(super) enum ChapterHead {
    /// `chapter_label` + a running number (filled in by `build`), with an
    /// optional subtitle from the folder's own title.
    Numbered(Option<String>),
    /// A verbatim heading — the folder's title, or a name like "Prologue".
    /// Carries no number and does not advance the count.
    Fixed(String),
}

/// Read a folder's `heading` override into a `ChapterHead`.
fn chapter_head(node: &Node) -> ChapterHead {
    match node.heading.trim() {
        "" | "numbered" => ChapterHead::Numbered(subtitle_of(&node.title)),
        "title" | "titled" if !node.title.trim().is_empty() => {
            ChapterHead::Fixed(node.title.trim().to_string())
        }
        "title" | "titled" => ChapterHead::Numbered(None),
        name => ChapterHead::Fixed(name.to_string()),
    }
}

/// Walk the tree into a flat list of parts and chapters. A single wrapper
/// folder (a "Manuscript" holding the chapters) is unwrapped; a folder holding
/// other folders is a part; a folder holding only text is a chapter; a bare
/// text node is a one-document chapter.
pub(super) fn structure(project: &Project) -> Vec<Item> {
    let fm = front_matter_folder(project);
    let is_body = |id: &NodeId| {
        Some(id) != fm.as_ref()
            && project.nodes.get(id).map(|n| n.include).unwrap_or(false)
    };

    // Find the level that actually holds the chapters, unwrapping lone folders.
    let mut level: Vec<NodeId> = project
        .children
        .get(ROOT)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(&is_body)
        .collect();
    loop {
        if level.len() != 1 {
            break;
        }
        let only = &level[0];
        let is_folder = project.nodes.get(only).map(|n| n.kind == Kind::Folder).unwrap_or(false);
        let kids: Vec<NodeId> = project.children.get(only).cloned().unwrap_or_default();
        let has_folder_child = kids
            .iter()
            .any(|k| project.nodes.get(k).map(|n| n.kind == Kind::Folder).unwrap_or(false));
        if is_folder && has_folder_child {
            level = kids.into_iter().filter(|k| project.nodes.get(k).map(|n| n.include).unwrap_or(false)).collect();
        } else {
            break;
        }
    }

    let mut items = Vec::new();
    for id in level {
        let Some(node) = project.nodes.get(&id) else { continue };
        if node.kind == Kind::Text {
            items.push(Item::Chapter { head: chapter_head(node), scenes: vec![id.clone()] });
            continue;
        }
        let kids: Vec<NodeId> = project
            .children
            .get(&id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|k| project.nodes.get(k).map(|n| n.include).unwrap_or(false))
            .collect();
        let has_folder = kids
            .iter()
            .any(|k| project.nodes.get(k).map(|n| n.kind == Kind::Folder).unwrap_or(false));
        if has_folder {
            items.push(Item::Part { title: node.title.clone() });
            for ch in kids {
                let Some(cn) = project.nodes.get(&ch) else { continue };
                let scenes = if cn.kind == Kind::Folder {
                    project.text_descendants(&ch).into_iter().filter(|s| *s != ch).collect()
                } else {
                    vec![ch.clone()]
                };
                items.push(Item::Chapter { head: chapter_head(cn), scenes });
            }
        } else {
            items.push(Item::Chapter { head: chapter_head(node), scenes: kids });
        }
    }
    items
}

/// A folder title that is just "Chapter 4" or "Seven" is scaffolding, not a
/// subtitle; anything else is shown under the chapter number.
pub(super) fn subtitle_of(title: &str) -> Option<String> {
    let t = title.trim();
    let lower = t.to_lowercase();
    let bare = lower.strip_prefix("chapter").map(|r| r.trim()).unwrap_or(&lower);
    let generic = bare.is_empty()
        || bare.chars().all(|c| c.is_ascii_digit())
        || WORD_NUMBERS.contains(&bare)
        || bare.chars().all(|c| "ivxlcdm".contains(c));
    if generic || t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

pub(super) const WORD_NUMBERS: &[&str] = &[
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
    "twenty", "thirty", "forty", "fifty",
];

pub(super) fn chapter_heading(book: &Book, n: u32) -> String {
    let label = book.chapter_label.trim();
    if label.is_empty() {
        spell(n)
    } else {
        format!("{label} {}", spell(n))
    }
}

pub(super) fn spell(n: u32) -> String {
    const ONES: [&str; 20] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen",
        "nineteen",
    ];
    const TENS: [&str; 10] = [
        "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    let word = match n {
        0..=19 => ONES[n as usize].to_string(),
        20..=99 => {
            let (t, o) = ((n / 10) as usize, (n % 10) as usize);
            if o == 0 {
                TENS[t].to_string()
            } else {
                format!("{}-{}", TENS[t], ONES[o])
            }
        }
        _ => return n.to_string(),
    };
    let mut c = word.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => word,
    }
}

// ---- prose -> Typst --------------------------------------------------------

/// Render a document body as Typst content: blank-line paragraphs, `::: center`
/// blocks, `\newpage`, scene-break rules, and `**bold**` / `*italic*`.
///
/// Emphasis is line-based (as in the editor), so wrapped lines of one paragraph
/// are joined with a space before the markers are read.
pub(super) fn render_prose(body: &str) -> String {
    let mut out = String::new();
    let mut buf: Vec<&str> = Vec::new();
    let mut center: Vec<&str> = Vec::new();
    let mut in_center = false;

    // A prose paragraph: wrapped lines join with a space.
    let flush = |buf: &mut Vec<&str>, out: &mut String| {
        if !buf.is_empty() {
            out.push_str(&paragraph(&buf.join(" ")));
            out.push_str("\n\n");
            buf.clear();
        }
    };

    for line in body.lines() {
        let t = line.trim();
        if t == crate::markup::CENTER_OPEN {
            flush(&mut buf, &mut out);
            in_center = true;
        } else if in_center && t == crate::markup::CENTER_CLOSE {
            in_center = false;
            out.push_str(&centered_block(&center));
            center.clear();
        } else if in_center {
            // A centred block is verse: every line break is deliberate.
            center.push(t);
        } else if t == crate::markup::PAGE_BREAK {
            flush(&mut buf, &mut out);
            out.push_str("#pagebreak(weak: true)\n\n");
        } else if t == "---" || t == "* * *" {
            flush(&mut buf, &mut out);
            out.push_str("#scenebreak\n\n");
        } else if t.is_empty() {
            flush(&mut buf, &mut out);
        } else {
            buf.push(t);
        }
    }
    flush(&mut buf, &mut out);
    if in_center {
        out.push_str(&centered_block(&center));
    }
    out
}

/// Verse: source line breaks become hard breaks, blank lines stanza gaps.
/// Surrounding blank lines are trimmed. Returns just the runs, no wrapper.
fn verse(lines: &[&str]) -> String {
    let first = lines.iter().position(|l| !l.is_empty());
    let last = lines.iter().rposition(|l| !l.is_empty());
    let (Some(first), Some(last)) = (first, last) else {
        return String::new();
    };
    let mut out = String::new();
    let mut prev_blank = true; // suppress a leading stanza gap
    let mut stanza_start = true; // first line of the current stanza
    for line in &lines[first..=last] {
        if line.is_empty() {
            prev_blank = true;
        } else {
            if prev_blank && !stanza_start {
                // A blank line is a stanza break: a real vertical gap, wider
                // than the line spacing within a stanza.
                out.push_str("\n#v(0.9em)\n");
                stanza_start = true;
            }
            if !stanza_start {
                out.push_str(" #linebreak()\n");
            }
            out.push_str(&paragraph(line));
            prev_blank = false;
            stanza_start = false;
        }
    }
    out
}

/// A `::: center` block: centred, unjustified, verse breaks kept.
fn centered_block(lines: &[&str]) -> String {
    let inner = verse(lines);
    if inner.is_empty() {
        String::new()
    } else {
        format!("#align(center, block[\n#set par(justify: false, first-line-indent: 0pt, leading: 0.62em)\n{inner}\n])\n\n")
    }
}

/// Whole body as centred verse for a generated dedication page taken from an
/// authored document; the caller supplies the alignment and styling.
pub(super) fn render_centered(body: &str) -> String {
    let lines: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| *l != crate::markup::CENTER_OPEN && *l != crate::markup::CENTER_CLOSE)
        .collect();
    verse(&lines)
}

/// One paragraph of prose as a Typst content block: emphasis becomes
/// `#strong` / `#emph`, everything else is a verbatim string so no character
/// in the prose can be read as Typst syntax.
pub(super) fn paragraph(text: &str) -> String {
    let mut out = String::new();
    for seg in crate::markup::parse_line(text) {
        match seg {
            Segment::Text(t) => out.push_str(&format!("#{}", s(&t))),
            Segment::Styled { bold, italic, text } => {
                let inner = format!("#{}", s(&text));
                let wrapped = match (bold, italic) {
                    (true, true) => format!("#strong[#emph[{inner}]]"),
                    (true, false) => format!("#strong[{inner}]"),
                    (false, true) => format!("#emph[{inner}]"),
                    (false, false) => inner,
                };
                out.push_str(&wrapped);
            }
        }
    }
    out
}

// ---- Typst literal helpers -------------------------------------------------

/// A Typst string literal: only `\` and `"` need escaping.
pub(super) fn s(text: &str) -> String {
    let mut o = String::with_capacity(text.len() + 2);
    o.push('"');
    for c in text.chars() {
        if c == '\\' || c == '"' {
            o.push('\\');
        }
        o.push(c);
    }
    o.push('"');
    o
}

/// A short bit of plain text as Typst content (inside `[...]`), via a string so
/// it cannot be markup.
pub(super) fn content(text: &str) -> String {
    format!("#{}", s(text))
}

/// Text destined for a spot where only Typst's own markup is safe (a heading
/// body, a `#let` string): strip anything structural, keep it plain.
pub(super) fn raw(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\\' | '#' | '[' | ']' | '*' | '_' | '$' | '<' | '>' | '@' | '`' => ' ',
            _ => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn trim_f(v: f32) -> String {
    if (v - v.round()).abs() < f32::EPSILON {
        format!("{}", v.round() as i64)
    } else {
        format!("{v}")
    }
}

/// Page dimensions in inches for a trim keyword.
pub(super) fn trim_size(trim: &str) -> (f32, f32) {
    match trim.trim().to_lowercase().as_str() {
        "5x8" => (5.0, 8.0),
        "5.25x8" => (5.25, 8.0),
        "6x9" => (6.0, 9.0),
        "a5" => (5.83, 8.27),
        _ => (5.5, 8.5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> Project {
        let dir = std::env::temp_dir().join(format!("jqln-lower-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = Project::create(&dir, "The Salt Road").unwrap();
        let roots: Vec<String> = p.children[ROOT].clone();
        for r in roots {
            p.remove(&r);
        }
        p
    }

    #[test]
    fn paragraph_emits_prose_as_a_string_so_syntax_cannot_leak() {
        let p = paragraph("a #hash, a **bold** word, and a [bracket]");
        assert!(p.contains(r#"#"a #hash, a ""#), "plain text must be a string: {p}");
        assert!(p.contains("#strong[#\"bold\"]"), "bold becomes #strong: {p}");
        assert!(p.contains(r#"#" word, and a [bracket]""#), "brackets stay inside a string: {p}");
    }

    #[test]
    fn render_prose_handles_breaks_and_centre_blocks() {
        let body = "First para.\n\nSecond para.\n\n\\newpage\n\n::: center\nCentred line\n:::\n\n* * *\n\nAfter.";
        let out = render_prose(body);
        assert!(out.contains("#pagebreak(weak: true)"));
        assert!(out.contains("#align(center, block["));
        assert!(out.contains("#scenebreak"));
    }

    #[test]
    fn a_centred_block_keeps_its_line_breaks_as_verse() {
        let body = "::: center\nRoses are red,\nviolets are blue,\n\nand so, it seems, are you.\n:::";
        let out = render_prose(body);
        // One hard break inside the first stanza, a vertical gap between stanzas,
        // and nothing joined onto one line.
        assert_eq!(out.matches("#linebreak()").count(), 1);
        assert!(out.contains("#v(0.9em)"), "blank line becomes a stanza gap");
        assert!(out.contains(r#"#"violets are blue,""#));
        assert!(!out.contains("violets are blue, and so"), "lines must not be joined");
    }

    #[test]
    fn subtitle_only_shows_for_real_titles() {
        assert_eq!(subtitle_of("Chapter 3"), None);
        assert_eq!(subtitle_of("Seven"), None);
        assert_eq!(subtitle_of("XIV"), None);
        assert_eq!(subtitle_of("The Salt Flats"), Some("The Salt Flats".to_string()));
    }

    #[test]
    fn structure_unwraps_a_lone_manuscript_folder() {
        let mut p = scratch("structure");
        let ms = p.insert(ROOT, None, "Manuscript", Kind::Folder);
        let c1 = p.insert(&ms, None, "Chapter One", Kind::Folder);
        p.insert(&c1, None, "Scene", Kind::Text);
        let c2 = p.insert(&ms, None, "The Reckoning", Kind::Folder);
        p.insert(&c2, None, "Scene", Kind::Text);

        let items = structure(&p);
        assert_eq!(items.len(), 2, "two chapters, manuscript wrapper unwrapped");
        match &items[1] {
            Item::Chapter { head: ChapterHead::Numbered(sub), .. } => {
                assert_eq!(sub.as_deref(), Some("The Reckoning"))
            }
            _ => panic!("expected a numbered chapter with a subtitle"),
        }
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn heading_override_maps_to_the_right_chapter_head() {
        let mut p = scratch("heads");
        let ms = p.insert(ROOT, None, "Manuscript", Kind::Folder);
        let pro = p.insert(&ms, None, "Prologue", Kind::Folder);
        p.nodes.get_mut(&pro).unwrap().heading = "Prologue".into();
        p.insert(&pro, None, "s", Kind::Text);
        let one = p.insert(&ms, None, "The Salt Flats", Kind::Folder);
        p.nodes.get_mut(&one).unwrap().heading = "title".into();
        p.insert(&one, None, "s", Kind::Text);
        let two = p.insert(&ms, None, "Chapter Two", Kind::Folder);
        p.insert(&two, None, "s", Kind::Text);

        let items = structure(&p);
        assert!(matches!(items[0], Item::Chapter { head: ChapterHead::Fixed(ref t), .. } if t == "Prologue"));
        assert!(matches!(items[1], Item::Chapter { head: ChapterHead::Fixed(ref t), .. } if t == "The Salt Flats"));
        assert!(matches!(items[2], Item::Chapter { head: ChapterHead::Numbered(None), .. }));
        let _ = std::fs::remove_dir_all(&p.root);
    }
}
