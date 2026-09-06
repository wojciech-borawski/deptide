use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{AppError, AppResult};

pub fn read_json<T: DeserializeOwned>(path: &Path) -> AppResult<Option<T>> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };

    serde_json::from_str(&raw).map(Some).map_err(|error| {
        AppError::with_context(&format!("Invalid JSON in {}", path.display()), error)
    })
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut body = serde_json::to_string_pretty(value)?;
    body.push('\n');
    fs::write(path, body)?;

    Ok(())
}
