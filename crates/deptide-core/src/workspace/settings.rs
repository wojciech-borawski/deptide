use super::Workspace;
use crate::domain::Settings;
use crate::error::AppResult;
use crate::util::json::{read_json, write_json};

pub fn load_settings(workspace: &Workspace) -> Settings {
    read_json::<Settings>(&workspace.settings_file())
        .ok()
        .flatten()
        .unwrap_or_default()
        .sanitized()
}

pub fn save_settings(workspace: &Workspace, settings: &Settings) -> AppResult<Settings> {
    let sanitized = settings.clone().sanitized();
    write_json(&workspace.settings_file(), &sanitized)?;
    Ok(sanitized)
}
