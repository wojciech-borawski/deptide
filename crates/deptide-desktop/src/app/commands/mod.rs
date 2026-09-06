mod misc;
mod run;
mod scan;
mod transfer;
mod workspace;

use deptide_core::error::{AppError, AppResult};

pub use misc::*;
pub use run::*;
pub use scan::*;
pub use transfer::*;
pub use workspace::*;

async fn on_blocking_thread<T, F>(work: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| AppError::new(error.to_string()))?
}
