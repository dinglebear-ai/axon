//! Process-wide lifecycle support for detached blocking workers.

use std::sync::Mutex;
use std::thread::JoinHandle;

#[derive(Default)]
struct State {
    handles: Vec<JoinHandle<()>>,
    draining: bool,
}

/// Owns detached thread handles and joins both previously registered workers
/// and workers submitted after draining begins.
#[derive(Default)]
pub struct DetachedWorkerRegistry {
    state: Mutex<State>,
}

impl DetachedWorkerRegistry {
    pub fn track(&self, worker: JoinHandle<()>) {
        let (finished, late_worker) = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut finished = Vec::new();
            let mut index = state.handles.len();
            while index > 0 {
                index -= 1;
                if state.handles[index].is_finished() {
                    finished.push(state.handles.swap_remove(index));
                }
            }
            if state.draining {
                (finished, Some(worker))
            } else {
                state.handles.push(worker);
                (finished, None)
            }
        };
        for worker in finished.into_iter().chain(late_worker) {
            Self::join(worker);
        }
    }

    pub fn drain(&self) {
        let workers = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.draining = true;
            std::mem::take(&mut state.handles)
        };
        for worker in workers {
            Self::join(worker);
        }
    }

    fn join(worker: JoinHandle<()>) {
        if worker.join().is_err() {
            tracing::error!("detached worker panicked");
        }
    }
}

#[cfg(test)]
#[path = "detached_workers_tests.rs"]
mod tests;
