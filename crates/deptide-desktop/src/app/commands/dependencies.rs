use super::on_blocking_thread;
use deptide_core::dependencies::{
    apply_latest_versions, fetch_latest_versions, find_installed_copies as find_copies,
    read_dependency_drift as read_drift, read_dependency_graph as read_graph, DependencyRequest,
    DependencyRequests, LatestLookup,
};
use deptide_core::domain::{CopyScope, DependencyDrift, DependencyGraph, ProjectCopies};
use deptide_core::error::{AppError, AppResult};
use deptide_core::workspace::open_with_config;
use tauri::{AppHandle, Manager, State};

const CANCELLED: &str = "The dependency request was cancelled";

struct RunningRequest {
    app: AppHandle,
    id: u64,
    request: DependencyRequest,
}

impl RunningRequest {
    fn start(app: AppHandle, id: u64) -> Self {
        let request = app.state::<DependencyRequests>().start(id);
        Self { app, id, request }
    }

    fn finish<T>(&self, value: T) -> AppResult<T> {
        if self.request.is_cancelled() {
            return Err(AppError::from(CANCELLED));
        }
        Ok(value)
    }
}

impl Drop for RunningRequest {
    fn drop(&mut self) {
        self.app
            .state::<DependencyRequests>()
            .finish(self.id, &self.request);
    }
}

#[tauri::command]
pub async fn read_dependency_drift(
    app: AppHandle,
    root: String,
    check_latest: bool,
    request_id: u64,
) -> AppResult<DependencyDrift> {
    let running = RunningRequest::start(app, request_id);
    let (workspace, config) = open_with_config(&root)?;
    let (workspace, config, mut drift) = on_blocking_thread(move || {
        let drift = read_drift(&workspace, &config);
        Ok((workspace, config, drift))
    })
    .await?;

    if check_latest {
        let lookup = LatestLookup::new(running.request.clone());
        let latest = fetch_latest_versions(&workspace, &config, &drift, &lookup).await;
        apply_latest_versions(&mut drift, &latest);
    }
    running.finish(drift)
}

#[tauri::command]
pub async fn read_dependency_graph(root: String) -> AppResult<DependencyGraph> {
    let (workspace, config) = open_with_config(&root)?;
    on_blocking_thread(move || Ok(read_graph(&workspace, &config))).await
}

#[tauri::command]
pub async fn find_installed_copies(
    app: AppHandle,
    root: String,
    scope: CopyScope,
    request_id: u64,
) -> AppResult<Vec<ProjectCopies>> {
    let running = RunningRequest::start(app, request_id);
    let (workspace, config) = open_with_config(&root)?;
    let request = running.request.clone();
    let copies =
        on_blocking_thread(move || Ok(find_copies(&workspace, &config, scope, &request))).await?;
    running.finish(copies)
}

#[tauri::command]
pub fn cancel_dependency_request(requests: State<'_, DependencyRequests>, request_id: u64) {
    requests.cancel(request_id);
}

#[tauri::command]
pub fn open_dependency_request(requests: State<'_, DependencyRequests>) -> u64 {
    requests.open()
}
