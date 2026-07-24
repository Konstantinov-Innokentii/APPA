pub mod server;
pub mod systems;

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
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
}
