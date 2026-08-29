//! Per-document snapshots: cheap copies of a document's text kept under
//! `snapshots/<id>/`, so going back on a rewrite is never a loss.

use super::Project;
use super::text::timestamp;
use std::path::PathBuf;

impl Project {
    pub fn snapshots_dir(&self, id: &str) -> PathBuf {
        self.root.join("snapshots").join(id)
    }

    /// Copy a document's current text aside under a timestamped name.
    pub fn take_snapshot(&mut self, id: &str) -> std::io::Result<String> {
        let body = self.body(id);
        let dir = self.snapshots_dir(id);
        std::fs::create_dir_all(&dir)?;
        let mut stamp = timestamp();
        // Two snapshots in the same second must not collide.
        let mut n = 1;
        while dir.join(format!("{stamp}.md")).exists() {
            stamp = format!("{}-{n}", timestamp());
            n += 1;
        }
        std::fs::write(dir.join(format!("{stamp}.md")), body)?;
        Ok(stamp)
    }

    pub fn delete_snapshot(&self, id: &str, name: &str) -> std::io::Result<()> {
        std::fs::remove_file(self.snapshots_dir(id).join(format!("{name}.md")))
    }

    /// Snapshot names for a document, newest first.
    pub fn list_snapshots(&self, id: &str) -> Vec<String> {
        let dir = self.snapshots_dir(id);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_str()?.to_string();
                name.strip_suffix(".md").map(|s| s.to_string())
            })
            .collect();
        names.sort_by(|a, b| b.cmp(a));
        names
    }

    /// Replace a document's text with a snapshot, after snapshotting what is
    /// there now so that restoring is never destructive.
    pub fn restore_snapshot(&mut self, id: &str, name: &str) -> std::io::Result<()> {
        let path = self.snapshots_dir(id).join(format!("{name}.md"));
        let text = std::fs::read_to_string(&path)?;
        self.take_snapshot(id)?;
        self.set_body(id, text);
        Ok(())
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
    fn snapshots_capture_and_restore_without_losing_work() {
        let dir = scratch("snap");
        let mut p = Project::create(&dir, "T").unwrap();
        let scene = p
            .walk()
            .into_iter()
            .map(|(i, _)| i)
            .find(|i| p.nodes[i].title == "Opening Scene")
            .unwrap();

        p.set_body(&scene, "First draft.".into());
        let first = p.take_snapshot(&scene).unwrap();
        assert_eq!(p.list_snapshots(&scene), std::slice::from_ref(&first));

        // Rewrite, then go back.
        p.set_body(&scene, "Rewritten and worse.".into());
        p.restore_snapshot(&scene, &first).unwrap();
        assert_eq!(p.body(&scene), "First draft.");

        // Restoring snapshotted the rewrite first, so nothing was lost.
        let all = p.list_snapshots(&scene);
        assert_eq!(all.len(), 2, "restore should preserve the replaced text");
        let saved: Vec<String> = all
            .iter()
            .map(|n| {
                std::fs::read_to_string(p.snapshots_dir(&scene).join(format!("{n}.md"))).unwrap()
            })
            .collect();
        assert!(saved.contains(&"Rewritten and worse.".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
