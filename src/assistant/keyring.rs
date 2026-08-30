//! Where the API key comes from: an environment variable, or a global config
//! file that is never inside a project.
//!
//!   1. `ANTHROPIC_API_KEY` / `OPENAI_API_KEY`
//!   2. `$XDG_CONFIG_HOME/jqln/config.toml` (or `~/.config/jqln/config.toml`,
//!      `%APPDATA%\jqln\config.toml` on Windows), refused if its Unix
//!      permissions are looser than `0600`.
//!
//! ```toml
//! [anthropic]
//! api_key = "sk-ant-…"
//! [openai]
//! api_key = "sk-…"
//! ```

use std::path::PathBuf;

/// The config-file path, if a home/config dir can be found.
pub fn config_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("jqln").join("config.toml"))
    }
    #[cfg(not(windows))]
    {
        if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(x).join("jqln").join("config.toml"));
        }
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".config").join("jqln").join("config.toml"))
    }
}

fn env_var(provider: &str) -> Option<String> {
    let key = match provider {
        "openai" => "OPENAI_API_KEY",
        _ => "ANTHROPIC_API_KEY",
    };
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

/// Read `[<provider>].api_key` from the global config file, enforcing `0600`.
fn file_key(provider: &str) -> Result<Option<String>, String> {
    let Some(path) = config_path() else { return Ok(None) };
    if !path.exists() {
        return Ok(None);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .permissions()
            .mode()
            & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!(
                "{} is readable by others (mode {mode:o}); run: chmod 600 {}",
                path.display(),
                path.display()
            ));
        }
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: toml::Table =
        text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(doc
        .get(provider)
        .and_then(|t| t.get("api_key"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty()))
}

/// The key for `provider`, or a sentence explaining where to put one.
pub fn resolve(provider: &str) -> Result<String, String> {
    if let Some(k) = env_var(provider) {
        return Ok(k);
    }
    match file_key(provider) {
        Ok(Some(k)) => Ok(k),
        Ok(None) => Err(missing(provider)),
        Err(e) => Err(e),
    }
}

fn missing(provider: &str) -> String {
    format!("no {provider} key — type /key to paste one, or set ${}", env_name(provider))
}

fn env_name(provider: &str) -> &'static str {
    if provider == "openai" { "OPENAI_API_KEY" } else { "ANTHROPIC_API_KEY" }
}

/// Write `key` into `[<provider>].api_key` of the global config file, creating
/// it (and its directory) and locking it to `0600`. Returns the path written.
pub fn save(provider: &str, key: &str) -> Result<PathBuf, String> {
    let path = config_path().ok_or("no config directory (set $HOME or $XDG_CONFIG_HOME)")?;
    save_to(&path, provider, key)?;
    Ok(path)
}

fn save_to(path: &std::path::Path, provider: &str, key: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }

    let mut doc: toml::Table = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default();

    if doc.get(provider).and_then(|v| v.as_table()).is_none() {
        doc.insert(provider.to_string(), toml::Value::Table(toml::Table::new()));
    }
    doc.get_mut(provider)
        .and_then(|v| v.as_table_mut())
        .unwrap()
        .insert("api_key".to_string(), toml::Value::String(key.trim().to_string()));

    let text = toml::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Serialises the tests that mutate process-global environment variables, so
/// they do not race each other (or the app tests that read the same vars).
#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_to_creates_merges_and_locks_the_file() {
        let dir = std::env::temp_dir().join(format!("jqln-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.toml");

        save_to(&path, "anthropic", "  sk-ant-abc  ").unwrap();
        save_to(&path, "openai", "sk-oai-xyz").unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("sk-ant-abc") && text.contains("sk-oai-xyz"), "{text}");

        let doc: toml::Table = text.parse().unwrap();
        assert_eq!(
            doc.get("anthropic").and_then(|t| t.get("api_key")).and_then(|v| v.as_str()),
            Some("sk-ant-abc")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_var_is_preferred_and_trimmed_blanks_are_ignored() {
        let _g = super::env_lock();
        // Not asserting a specific key (CI has none) — just that a blank is rejected.
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "   ");
        }
        assert!(env_var("anthropic").is_none());
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-xyz");
        }
        assert_eq!(env_var("anthropic").as_deref(), Some("sk-ant-xyz"));
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }
    }
}
