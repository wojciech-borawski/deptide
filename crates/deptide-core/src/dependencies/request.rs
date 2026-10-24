use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::execution::ProcessRegistry;

const LATEST_LOOKUPS: usize = 4;
const LATEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Default)]
pub struct DependencyRequest {
    processes: Arc<ProcessRegistry>,
}

impl DependencyRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.processes.abort_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.processes.is_aborted()
    }

    pub(super) fn processes(&self) -> &ProcessRegistry {
        &self.processes
    }

    fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.processes, &other.processes)
    }
}

#[derive(Debug, Clone)]
pub struct LatestLookup {
    pub limit: usize,
    pub timeout: Duration,
    pub request: DependencyRequest,
}

impl LatestLookup {
    pub fn new(request: DependencyRequest) -> Self {
        Self {
            limit: LATEST_LOOKUPS,
            timeout: LATEST_TIMEOUT,
            request,
        }
    }
}

#[derive(Debug, Default)]
pub struct DependencyRequests {
    table: Mutex<RequestTable>,
}

#[derive(Debug, Default)]
struct RequestTable {
    last_id: u64,
    opened: HashMap<u64, DependencyRequest>,
    running: HashMap<u64, DependencyRequest>,
}

impl DependencyRequests {
    pub fn open(&self) -> u64 {
        let mut table = self.lock();
        table.last_id += 1;
        let id = table.last_id;
        table.opened.insert(id, DependencyRequest::new());
        id
    }

    pub fn start(&self, id: u64) -> DependencyRequest {
        let mut table = self.lock();
        match table.opened.remove(&id) {
            Some(request) => {
                table.running.insert(id, request.clone());
                request
            }
            None => {
                let refused = DependencyRequest::new();
                refused.cancel();
                refused
            }
        }
    }

    pub fn finish(&self, id: u64, request: &DependencyRequest) {
        let mut table = self.lock();
        let owns_entry = table
            .running
            .get(&id)
            .is_some_and(|running| running.is_same(request));
        if owns_entry {
            table.running.remove(&id);
        }
    }

    pub fn cancel(&self, id: u64) {
        let mut table = self.lock();
        let request = table
            .opened
            .remove(&id)
            .or_else(|| table.running.remove(&id));
        if let Some(request) = request {
            request.cancel();
        }
    }

    pub fn cancel_all(&self) {
        let mut table = self.lock();
        let RequestTable {
            opened, running, ..
        } = &mut *table;
        opened
            .drain()
            .chain(running.drain())
            .for_each(|(_, request)| request.cancel());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RequestTable> {
        self.table
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
