//! Turning an assistant reply's `jqln-comments` block into inline `{>>…<<}`
//! comments on the current document.

use serde::Deserialize;

/// One remark the assistant wants to anchor.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Proposal {
    /// An exact run of text from the current document.
    pub quote: String,
    /// The remark itself.
    pub note: String,
}

/// Pull the first ` ```jqln-comments … ``` ` block out of a reply and parse it.
/// Returns `(proposals, reply_without_the_block)`.
pub fn extract(reply: &str) -> (Vec<Proposal>, String) {
    let Some(start) = reply.find("```jqln-comments") else {
        return (Vec::new(), reply.to_string());
    };
    let after = &reply[start + "```jqln-comments".len()..];
    let Some(end_rel) = after.find("```") else {
        return (Vec::new(), reply.to_string());
    };
    let json = after[..end_rel].trim();
    let proposals: Vec<Proposal> = serde_json::from_str(json).unwrap_or_default();

    let mut cleaned = reply[..start].trim_end().to_string();
    let tail = after[end_rel + 3..].trim_start();
    if !tail.is_empty() {
        if !cleaned.is_empty() {
            cleaned.push_str("\n\n");
        }
        cleaned.push_str(tail);
    }
    (proposals, cleaned)
}

/// Outcome of trying to place one proposal.
pub enum Placement {
    /// Byte offsets in `body` where `{==quote==}` should wrap.
    At(usize, usize),
    /// The quote was not found.
    Missing,
    /// The quote appears more than once — too risky to guess.
    Ambiguous,
}

/// Where in `body` this proposal's quote sits.
pub fn place(body: &str, quote: &str) -> Placement {
    let q = quote.trim();
    if q.is_empty() {
        return Placement::Missing;
    }
    let mut hits = body.match_indices(q);
    let Some((first, _)) = hits.next() else {
        return Placement::Missing;
    };
    if hits.next().is_some() {
        return Placement::Ambiguous;
    }
    // A quote that straddles a line break can't be wrapped inline.
    if body[first..first + q.len()].contains('\n') {
        return Placement::Missing;
    }
    Placement::At(first, first + q.len())
}

/// Apply the placeable proposals to `body`, wrapping each quote as
/// `{==quote==}{>>prefix note<<}`. Returns the new body and a per-proposal
/// note of what happened. Applied right-to-left so earlier offsets stay valid.
pub fn apply(body: &str, prefix: &str, proposals: &[Proposal]) -> (String, Vec<String>) {
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut report: Vec<String> = Vec::new();

    for p in proposals {
        match place(body, &p.quote) {
            Placement::At(s, e) => {
                let note = p.note.replace(['\n', '\r'], " ");
                edits.push((s, e, format!("{{=={}==}}{{>>{prefix}{note}<<}}", &body[s..e])));
                report.push(format!("✓ “{}”", short(&p.quote)));
            }
            Placement::Missing => report.push(format!("✗ not found: “{}”", short(&p.quote))),
            Placement::Ambiguous => {
                report.push(format!("✗ appears more than once: “{}”", short(&p.quote)))
            }
        }
    }

    edits.sort_by_key(|(s, _, _)| *s);
    // Drop any edit that overlaps an earlier one.
    let mut kept: Vec<(usize, usize, String)> = Vec::new();
    for e in edits {
        if kept.last().is_none_or(|last| e.0 >= last.1) {
            kept.push(e);
        }
    }

    let mut out = String::with_capacity(body.len() + 64);
    let mut cursor = 0;
    for (s, e, replacement) in &kept {
        out.push_str(&body[cursor..*s]);
        out.push_str(replacement);
        cursor = *e;
    }
    out.push_str(&body[cursor..]);
    (out, report)
}

fn short(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= 40 {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(39).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_block_and_leaves_prose() {
        let reply = "Here are some notes.\n\n```jqln-comments\n[{\"quote\":\"the road\",\"note\":\"vague\"}]\n```\n";
        let (props, cleaned) = extract(reply);
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].quote, "the road");
        assert_eq!(cleaned, "Here are some notes.");
    }

    #[test]
    fn no_block_returns_reply_unchanged() {
        let (props, cleaned) = extract("just talking");
        assert!(props.is_empty());
        assert_eq!(cleaned, "just talking");
    }

    #[test]
    fn applies_matches_and_reports_the_rest() {
        let body = "The road was long. The sky was grey.";
        let props = vec![
            Proposal { quote: "The road was long".into(), note: "flat open".into() },
            Proposal { quote: "the mountains".into(), note: "n/a".into() },
        ];
        let (out, report) = apply(body, "AI: ", &props);
        assert_eq!(out, "{==The road was long==}{>>AI: flat open<<}. The sky was grey.");
        assert!(report[0].starts_with('✓'));
        assert!(report[1].contains("not found"));
    }

    #[test]
    fn ambiguous_quote_is_skipped() {
        let body = "go. go. go.";
        let props = vec![Proposal { quote: "go".into(), note: "x".into() }];
        let (out, report) = apply(body, "", &props);
        assert_eq!(out, body);
        assert!(report[0].contains("more than once"));
    }
}
