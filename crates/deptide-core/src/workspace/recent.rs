use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::util::json::{read_json, write_json};
use crate::util::time::iso_timestamp;

const RECENT_FILE_NAME: &str = "recent-workspaces.json";
const MAX_RECENT: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentWorkspace {
    pub path: String,
    pub last_opened: String,
}

pub fn list_recent(app_config_dir: &Path) -> Vec<RecentWorkspace> {
    read_json::<Vec<RecentWorkspace>>(&app_config_dir.join(RECENT_FILE_NAME))
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| Path::new(&entry.path).is_dir())
        .collect()
}

pub fn remember_recent(app_config_dir: &Path, path: &str) -> AppResult<Vec<RecentWorkspace>> {
    let mut entries: Vec<RecentWorkspace> = list_recent(app_config_dir)
        .into_iter()
        .filter(|entry| !same_path(&entry.path, path))
        .collect();

    entries.insert(
        0,
        RecentWorkspace {
            path: path.to_string(),
            last_opened: iso_timestamp(Utc::now()),
        },
    );
    entries.truncate(MAX_RECENT);

    write_json(&app_config_dir.join(RECENT_FILE_NAME), &entries)?;
    Ok(entries)
}

fn same_path(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}
