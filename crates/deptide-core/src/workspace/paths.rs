use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

pub const SETTINGS_FILE_NAME: &str = "settings.json";
pub const CONFIG_FILE_NAME: &str = "update-libs.json";
pub const LEGACY_CONFIG_DIRECTORY: &str = "config";
pub const RUNS_DIRECTORY: &str = "runs";
pub const LOGS_DIRECTORY: &str = "logs";
pub const BACKUPS_DIRECTORY: &str = "backups";
pub const TRANSFERS_DIRECTORY: &str = "transfers";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn open(root: impl AsRef<Path>) -> AppResult<Self> {
        let root = lexical_normalize(root.as_ref());

        if !root.is_dir() {
            return Err(AppError::new(format!("Not a folder: {}", root.display())));
        }

        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join(SETTINGS_FILE_NAME)
    }

    pub fn config_file(&self) -> PathBuf {
        let primary = self.root.join(CONFIG_FILE_NAME);
        let legacy = self
            .root
            .join(LEGACY_CONFIG_DIRECTORY)
            .join(CONFIG_FILE_NAME);

        if !primary.exists() && legacy.exists() {
            legacy
        } else {
            primary
        }
    }

    pub fn config_directory(&self) -> PathBuf {
        self.config_file()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.root.clone())
    }

    pub fn runs_directory(&self) -> PathBuf {
        self.root.join(RUNS_DIRECTORY)
    }

    pub fn backups_directory(&self) -> PathBuf {
        self.root.join(BACKUPS_DIRECTORY)
    }

    pub fn transfers_directory(&self) -> PathBuf {
        self.root.join(TRANSFERS_DIRECTORY)
    }

    pub fn logs_directory(&self) -> PathBuf {
        self.root.join(LOGS_DIRECTORY)
    }

    pub fn resolve_project(&self, configured_path: &str) -> PathBuf {
        lexical_normalize(&self.config_directory().join(configured_path))
    }

    pub fn to_config_path(&self, target: &Path) -> String {
        to_config_path(&self.config_directory(), target)
    }
}

pub fn to_config_path(from_directory: &Path, target: &Path) -> String {
    let target = lexical_normalize(target);

    match relative_path(&lexical_normalize(from_directory), &target) {
        Some(relative) => {
            let posix = to_posix(&relative);
            if posix.is_empty() {
                ".".to_string()
            } else if posix.starts_with('.') {
                posix
            } else {
                format!("./{posix}")
            }
        }
        None => to_posix(&target),
    }
}

pub fn to_posix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn lexical_normalize(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    let mut normalized = PathBuf::new();

    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(std::path::MAIN_SEPARATOR_STR),
            Component::CurDir => {}
            Component::ParentDir => {
                let is_root = matches!(
                    normalized.components().next_back(),
                    Some(Component::RootDir) | Some(Component::Prefix(_)) | None
                );
                if !is_root {
                    normalized.pop();
                }
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }

    normalized
}

pub fn normalize_directory(path: &Path) -> String {
    let normalized = lexical_normalize(path).to_string_lossy().to_string();

    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

pub fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_parts: Vec<Component> = from.components().collect();
    let to_parts: Vec<Component> = to.components().collect();

    let from_prefix = from_parts.first().copied();
    let to_prefix = to_parts.first().copied();

    if let (Some(Component::Prefix(a)), Some(Component::Prefix(b))) = (from_prefix, to_prefix) {
        if !same_prefix(a.as_os_str(), b.as_os_str()) {
            return None;
        }
    }

    let common = from_parts
        .iter()
        .zip(to_parts.iter())
        .take_while(|(a, b)| same_component(a, b))
        .count();

    let mut relative = PathBuf::new();
    for _ in common..from_parts.len() {
        relative.push("..");
    }
    for part in &to_parts[common..] {
        relative.push(part.as_os_str());
    }

    Some(relative)
}

fn same_component(a: &Component, b: &Component) -> bool {
    if cfg!(windows) {
        a.as_os_str().to_string_lossy().to_lowercase()
            == b.as_os_str().to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

fn same_prefix(a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}
