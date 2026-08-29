//! Full-text search across titles, synopses and prose.
//!
//! Queries are plain text by default — prose is full of brackets and full
//! stops that would otherwise read as regex syntax — and opt into a regular
//! expression by being wrapped in slashes: `/pattern/`.

use super::{Kind, NodeId, Project};

/// One matching line inside one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub id: NodeId,
    /// 1-based line number, or 0 when the title itself matched.
    pub line: usize,
    pub preview: String,
}

/// How a query is matched. Plain text by default; `/pattern/` opts into a
/// regular expression, since prose is full of characters that would otherwise
/// be read as syntax.
enum Matcher {
    Literal(String),
    Regex(regex::Regex),
}

impl Matcher {
    fn build(query: &str) -> Result<Matcher, String> {
        let q = query.trim();
        if q.len() >= 2 && q.starts_with('/') && q.ends_with('/') {
            let pattern = &q[1..q.len() - 1];
            if pattern.is_empty() {
                return Err("empty pattern".to_string());
            }
            regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map(Matcher::Regex)
                .map_err(|e| {
                    // regex errors are multi-line; the first line is the useful bit.
                    e.to_string().lines().next().unwrap_or("invalid pattern").to_string()
                })
        } else {
            Ok(Matcher::Literal(q.to_lowercase()))
        }
    }

    fn matches(&self, haystack: &str) -> bool {
        match self {
            Matcher::Literal(l) => haystack.to_lowercase().contains(l),
            Matcher::Regex(r) => r.is_match(haystack),
        }
    }
}

impl Project {
    /// Search titles, synopses and prose. Case-insensitive either way.
    pub fn search(&mut self, query: &str) -> Result<Vec<Hit>, String> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let needle = Matcher::build(query)?;
        let ids: Vec<NodeId> = self.walk().into_iter().map(|(i, _)| i).collect();
        let mut hits = Vec::new();

        for id in ids {
            let (title, synopsis, is_text) = match self.nodes.get(&id) {
                Some(n) => (n.title.clone(), n.synopsis.clone(), n.kind == Kind::Text),
                None => continue,
            };
            if needle.matches(&title) {
                hits.push(Hit { id: id.clone(), line: 0, preview: title.clone() });
            }
            if needle.matches(&synopsis) {
                hits.push(Hit { id: id.clone(), line: 0, preview: synopsis });
            }
            if !is_text {
                continue;
            }
            let body = self.body(&id);
            for (n, line) in body.lines().enumerate() {
                if needle.matches(line) {
                    hits.push(Hit {
                        id: id.clone(),
                        line: n + 1,
                        preview: line.trim().chars().take(160).collect(),
                    });
                }
            }
        }
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use crate::project::Project;

    fn scratch(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("jqln-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn search_finds_titles_synopses_and_prose() {
        let dir = scratch("search");
        let mut p = Project::create(&dir, "T").unwrap();
        let scene = p
            .walk()
            .into_iter()
            .map(|(i, _)| i)
            .find(|i| p.nodes[i].title == "Opening Scene")
            .unwrap();
        p.set_body(&scene, "The salt flats.\nNothing here.\nMore salt.".into());

        let hits = p.search("salt").unwrap();
        assert_eq!(hits.len(), 2, "two prose lines contain the word");
        assert_eq!(hits[0].line, 1);
        assert_eq!(hits[1].line, 3);

        // Titles match too, reported as line 0.
        let hits = p.search("research").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 0);

        // Case-insensitive, and blank queries return nothing.
        assert_eq!(p.search("SALT").unwrap().len(), 2);
        assert!(p.search("   ").unwrap().is_empty());
        assert!(p.search("absent").unwrap().is_empty());

        // A slash-delimited query is a regular expression.
        assert_eq!(p.search("/s[ae]lt/").unwrap().len(), 2);
        assert_eq!(p.search("/^more/").unwrap().len(), 1);
        // Plain text is matched literally, so metacharacters are harmless.
        assert!(p.search("s[ae]lt").unwrap().is_empty());
        // A broken pattern reports rather than silently finding nothing.
        assert!(p.search("/salt(/").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
