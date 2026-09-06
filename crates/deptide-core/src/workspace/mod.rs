mod config_file;
mod paths;
mod recent;
mod run_store;
mod settings;

pub use config_file::{load_config, parse_config_text, save_config};
pub use paths::{lexical_normalize, normalize_directory, relative_path, to_posix, Workspace};
pub use recent::{list_recent, remember_recent, RecentWorkspace};
pub use run_store::{delete_run, list_runs, load_run, save_run, suggest_run_name, SavedRunFile};
pub use settings::{load_settings, save_settings};

use crate::domain::UpdateConfig;
use crate::error::AppResult;

pub fn open_with_config(root: &str) -> AppResult<(Workspace, UpdateConfig)> {
    let workspace = Workspace::open(root)?;
    let config = load_config(&workspace)?;
    Ok((workspace, config))
}
