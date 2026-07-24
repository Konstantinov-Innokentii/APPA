pub mod fork_tools;

use std::path::PathBuf;

pub fn resolve_data_root(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(path) = explicit {
        return path;
    }
    if let Ok(env) = std::env::var("CORP_DATA_ROOT")
        && !env.trim().is_empty()
    {
        return PathBuf::from(env);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corp-systems/data")
}

pub fn resolve_sink_root(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(path) = explicit {
        return path;
    }
    if let Ok(env) = std::env::var("CORP_SINK_ROOT")
        && !env.trim().is_empty()
    {
        return PathBuf::from(env);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
}

pub fn clean_key(raw: &str) -> String {
    let t = raw.trim();
    let t = t.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(t);
    let t = t.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')).unwrap_or(t);
    t.to_string()
}

pub fn load_dotenv() -> Option<PathBuf> {
    let mut first_found = None;
    for candidate in [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.env"),
    ] {
        if let Ok(text) = std::fs::read_to_string(&candidate) {
            apply_env_file(&text);
            first_found.get_or_insert(candidate);
        }
    }
    first_found
}

fn apply_env_file(text: &str) {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();

        if key.is_empty() || std::env::var_os(key).is_some() {
            continue;
        }

        unsafe {
            std::env::set_var(key, clean_key(value));
        }
    }
}
