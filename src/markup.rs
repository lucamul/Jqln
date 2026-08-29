//! The small set of formatting Jqln understands, stored as plain Markdown so
//! the files stay readable in any other tool.
//!
//! Inline: `**bold**`, `*italic*`, and `***bold italic***`. Block: a line of
//! `::: center` / `:::` around centred text, and a lone `\newpage` for a page
//! break. Nothing here is a private marker — it is all Markdown that a
//! compiler downstream already knows how to read.
//!
//! Two jobs live here, both pure so they can be tested without a terminal:
//! turning a line into the highlight ranges the editor should paint, and
//! toggling a marker around a stretch of selected text.

use ratatui::style::{Color, Modifier, Style};
use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

/// A lone line carrying this text is a page break.
pub const PAGE_BREAK: &str = r"\newpage";
/// Opens a centred block; the matching close is a bare `:::`.
pub const CENTER_OPEN: &str = "::: center";
pub const CENTER_CLOSE: &str = ":::";

/// One highlight to hand to `TextArea::custom_highlight`: a `((row, col), (row,
/// col))` byte range, a style, and a priority. Priorities sit below the
/// editor's own selection (10) and search (20) layers so those still win.
pub type Highlight = (((usize, usize), (usize, usize)), Style, u8);

const CONTENT_PRIORITY: u8 = 1;
const MARKER_PRIORITY: u8 = 2;

fn inline_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Left to right, non-overlapping. Longest marker first so `***x***`
        // is read as bold+italic rather than bold followed by a stray `*`.
        Regex::new(r"\*\*\*([^*]+)\*\*\*|\*\*([^*]+)\*\*|\*([^*]+)\*").unwrap()
    })
}

fn faded() -> Style {
    Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM)
}

/// Every highlight the editor should paint for these lines.
pub fn highlights(lines: &[String]) -> Vec<Highlight> {
    let mut out = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == PAGE_BREAK || trimmed == CENTER_OPEN || trimmed == CENTER_CLOSE {
            // A structural line recedes as a whole; it is not prose.
            out.push((((row, 0), (row, line.len())), faded(), MARKER_PRIORITY));
            continue;
        }
        for caps in inline_re().captures_iter(line) {
            let whole = caps.get(0).unwrap();
            let (mlen, modifier) = if caps.get(1).is_some() {
                (3, Modifier::BOLD | Modifier::ITALIC)
            } else if caps.get(2).is_some() {
                (2, Modifier::BOLD)
            } else {
                (1, Modifier::ITALIC)
            };
            let (start, end) = (whole.start(), whole.end());
            // The content between the markers carries the style.
            out.push((
                ((row, start + mlen), (row, end - mlen)),
                Style::default().add_modifier(modifier),
                CONTENT_PRIORITY,
            ));
            // The markers themselves fade back.
            out.push((((row, start), (row, start + mlen)), faded(), MARKER_PRIORITY));
            out.push((((row, end - mlen), (row, end)), faded(), MARKER_PRIORITY));
        }
    }
    out
}

/// One run of a line, once the emphasis markers have been read off. Used by
/// exporters that need the structure rather than the raw asterisks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Styled { bold: bool, italic: bool, text: String },
}

/// Break a line into plain and emphasised runs. `**x**` is bold, `*x*` italic,
/// `***x***` both. A lone `*` or an unmatched pair stays literal.
pub fn parse_line(line: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut last = 0;
    for caps in inline_re().captures_iter(line) {
        let whole = caps.get(0).unwrap();
        if whole.start() > last {
            out.push(Segment::Text(line[last..whole.start()].to_string()));
        }
        let (bold, italic, text) = if let Some(m) = caps.get(1) {
            (true, true, m.as_str())
        } else if let Some(m) = caps.get(2) {
            (true, false, m.as_str())
        } else {
            (false, true, caps.get(3).unwrap().as_str())
        };
        out.push(Segment::Styled { bold, italic, text: text.to_string() });
        last = whole.end();
    }
    if last < line.len() {
        out.push(Segment::Text(line[last..].to_string()));
    }
    out
}

/// Toggle `marker` (`*` or `**`) around the byte range `start..end` of `line`.
/// Returns the rewritten line and the byte range that still covers the same
/// text, so the caller can keep it selected.
///
/// Three shapes are recognised, in order: markers sitting just outside the
/// selection (strip them), markers inside the selection (strip them), or
/// neither (wrap the selection).
pub fn toggle_inline(line: &str, start: usize, end: usize, marker: &str) -> (String, usize, usize) {
    let mlen = marker.len();
    let before = &line[..start];
    let after = &line[end..];
    let sel = &line[start..end];

    if marker_ends(before, marker) && marker_starts(after, marker) {
        let mut s = String::with_capacity(line.len());
        s.push_str(&before[..before.len() - mlen]);
        s.push_str(sel);
        s.push_str(&after[mlen..]);
        return (s, start - mlen, end - mlen);
    }

    if sel.len() >= 2 * mlen
        && marker_starts(sel, marker)
        && marker_ends(&sel[mlen..], marker)
    {
        let inner = &sel[mlen..sel.len() - mlen];
        let mut s = String::with_capacity(line.len());
        s.push_str(before);
        s.push_str(inner);
        s.push_str(after);
        return (s, start, end - 2 * mlen);
    }

    let mut s = String::with_capacity(line.len() + 2 * mlen);
    s.push_str(before);
    s.push_str(marker);
    s.push_str(sel);
    s.push_str(marker);
    s.push_str(after);
    (s, start + mlen, end + mlen)
}

/// True when `s` ends with exactly `marker` and not a longer run of `*`, so
/// `*` does not match the tail of a `**` pair.
fn marker_ends(s: &str, marker: &str) -> bool {
    s.ends_with(marker) && !s[..s.len() - marker.len()].ends_with('*')
}

fn marker_starts(s: &str, marker: &str) -> bool {
    s.starts_with(marker) && !s[marker.len()..].starts_with('*')
}

/// The prose with its formatting removed: inline markers dropped, structural
/// lines (`\newpage`, centre fences) taken out entirely. Used so markup does
/// not inflate a word count. Borrows straight back when there is nothing to do.
pub fn strip(body: &str) -> Cow<'_, str> {
    if !body.contains('*') && !body.contains(PAGE_BREAK) && !body.contains(CENTER_CLOSE) {
        return Cow::Borrowed(body);
    }
    let out = body
        .lines()
        .filter(|l| {
            let t = l.trim();
            t != PAGE_BREAK && t != CENTER_OPEN && t != CENTER_CLOSE
        })
        .map(|l| inline_re().replace_all(l, "$1$2$3").into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    Cow::Owned(out)
}

/// Byte offset of character column `col` in `line` (clamped to the end).
pub fn byte_index(line: &str, col: usize) -> usize {
    line.char_indices().nth(col).map(|(i, _)| i).unwrap_or(line.len())
}

/// Character column of byte offset `byte` in `line`.
pub fn char_index(line: &str, byte: usize) -> usize {
    line[..byte.min(line.len())].chars().count()
}

/// The word surrounding character column `col`, as a `[start, end)` column
/// range. Returns an empty range when the cursor is not on a word.
pub fn word_bounds(line: &str, col: usize) -> (usize, usize) {
    let chars: Vec<char> = line.chars().collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '’';
    let mut start = col.min(chars.len());
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = col.min(chars.len());
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.split('\n').map(|l| l.to_string()).collect()
    }

    #[test]
    fn wraps_and_unwraps_a_selection() {
        // Wrap.
        let (out, s, e) = toggle_inline("the salt road", 4, 8, "**");
        assert_eq!(out, "the **salt** road");
        assert_eq!(&out[s..e], "salt");

        // Selecting the same word again, now inside the markers, unwraps it.
        let (out, s, e) = toggle_inline("the **salt** road", 6, 10, "**");
        assert_eq!(out, "the salt road");
        assert_eq!(&out[s..e], "salt");
    }

    #[test]
    fn unwraps_when_the_markers_are_inside_the_selection() {
        let (out, s, e) = toggle_inline("a *word* b", 2, 8, "*");
        assert_eq!(out, "a word b");
        assert_eq!(&out[s..e], "word");
    }

    #[test]
    fn italic_toggle_leaves_a_bold_pair_alone() {
        // The chars just outside are `**`, not a lone `*`: this must wrap, not
        // strip half of the bold markers.
        let (out, _, _) = toggle_inline("x **salt** y", 4, 8, "*");
        assert_eq!(out, "x ***salt*** y");
    }

    #[test]
    fn highlights_cover_content_and_fade_markers() {
        let hl = highlights(&lines("the **salt** road"));
        // One content span plus two marker spans.
        assert_eq!(hl.len(), 3);
        let content = hl.iter().find(|(_, _, p)| *p == CONTENT_PRIORITY).unwrap();
        assert_eq!(content.0, ((0, 6), (0, 10)));
        assert!(content.1.add_modifier.contains(Modifier::BOLD));
        // Markers sit at 4..6 and 10..12.
        let markers: Vec<_> = hl.iter().filter(|(_, _, p)| *p == MARKER_PRIORITY).collect();
        assert_eq!(markers.len(), 2);
    }

    #[test]
    fn triple_marker_is_bold_and_italic() {
        let hl = highlights(&lines("say ***now*** please"));
        let content = hl.iter().find(|(_, _, p)| *p == CONTENT_PRIORITY).unwrap();
        assert!(content.1.add_modifier.contains(Modifier::BOLD));
        assert!(content.1.add_modifier.contains(Modifier::ITALIC));
        assert_eq!(content.0, ((0, 7), (0, 10)));
    }

    #[test]
    fn structural_lines_are_faded_whole() {
        let hl = highlights(&lines("::: center\nHere\n:::\n\\newpage"));
        // Fences on row 0 and row 2, page break on row 3. "Here" is untouched.
        let rows: Vec<usize> = hl.iter().map(|((( r, _), _), _, _)| *r).collect();
        assert_eq!(rows, vec![0, 2, 3]);
    }

    #[test]
    fn parse_line_splits_plain_and_styled_runs() {
        use Segment::*;
        assert_eq!(
            parse_line("a **bold** and *thin* end"),
            vec![
                Text("a ".into()),
                Styled { bold: true, italic: false, text: "bold".into() },
                Text(" and ".into()),
                Styled { bold: false, italic: true, text: "thin".into() },
                Text(" end".into()),
            ]
        );
        assert_eq!(
            parse_line("***both***"),
            vec![Styled { bold: true, italic: true, text: "both".into() }]
        );
        assert_eq!(parse_line("no markup here"), vec![Text("no markup here".into())]);
    }

    #[test]
    fn strip_removes_markers_and_structural_lines() {
        assert_eq!(strip("plain prose"), "plain prose"); // untouched, borrowed
        assert_eq!(strip("a **bold** and *thin* word"), "a bold and thin word");
        assert_eq!(
            strip("one\n\\newpage\n::: center\ntwo\n:::"),
            "one\ntwo"
        );
    }

    #[test]
    fn word_bounds_find_the_surrounding_word() {
        assert_eq!(word_bounds("the salt road", 6), (4, 8)); // inside "salt"
        assert_eq!(word_bounds("the salt road", 3), (0, 3)); // at the end of "the"
        assert_eq!(word_bounds("a  b", 2), (2, 2)); // adrift between two spaces
        assert_eq!(word_bounds("don't stop", 2), (0, 5)); // apostrophe joins
    }

    #[test]
    fn byte_and_char_index_round_trip_through_multibyte() {
        let line = "café **au** lait";
        let b = byte_index(line, 5); // start of "**"
        assert_eq!(&line[b..b + 2], "**");
        assert_eq!(char_index(line, b), 5);
    }
}
