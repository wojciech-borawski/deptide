use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use deptide_core::execution::RunContext;
use deptide_core::util::time::now_ms;

#[derive(Default)]
pub struct AppState {
    runs: Mutex<HashMap<String, Arc<RunContext>>>,
    counter: AtomicU64,
}

impl AppState {
    pub fn next_run_id(&self) -> String {
        let sequence = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        format!("run-{}-{sequence}", now_ms())
    }

    pub fn register(&self, context: Arc<RunContext>) {
        self.runs().insert(context.id.clone(), context);
    }

    pub fn get(&self, run_id: &str) -> Option<Arc<RunContext>> {
        self.runs().get(run_id).cloned()
    }

    pub fn active_run(&self) -> Option<Arc<RunContext>> {
        self.runs()
            .values()
            .find(|context| !context.is_finished())
            .cloned()
    }

    pub fn abort_all(&self) {
        for context in self.runs().values() {
            context.abort();
        }
    }

    fn runs(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<RunContext>>> {
        self.runs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
