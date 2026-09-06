use std::time::Duration;

use serde::{Deserialize, Serialize};

use deptide_core::error::{AppError, AppResult};
use deptide_core::util::version::is_newer;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub url: Option<String>,
    pub notes: Option<String>,
}

pub fn check(url: &str) -> AppResult<UpdateInfo> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent();

    let manifest: Manifest = agent
        .get(url)
        .call()
        .map_err(|error| AppError::with_context("Update check failed", error))?
        .body_mut()
        .read_json()
        .map_err(|error| AppError::with_context("Update manifest is not valid JSON", error))?;

    Ok(UpdateInfo {
        current: CURRENT_VERSION.to_string(),
        newer: is_newer(&manifest.version, CURRENT_VERSION),
        latest: manifest.version,
        url: manifest.url,
        notes: manifest.notes,
    })
}
