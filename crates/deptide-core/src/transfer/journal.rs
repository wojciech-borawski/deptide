use chrono::Utc;
use serde::Serialize;

use crate::util::json::write_json;
use crate::util::time::file_stamp;
use crate::workspace::Workspace;

pub fn record_transfer<T: Serialize>(
    workspace: &Workspace,
    kind: &str,
    record: &T,
) -> Option<String> {
    let file = workspace
        .transfers_directory()
        .join(format!("{}-{kind}.json", file_stamp(Utc::now())));
    write_json(&file, record)
        .ok()
        .map(|_| file.to_string_lossy().to_string())
}
