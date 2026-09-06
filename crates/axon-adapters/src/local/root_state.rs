use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axon_api::source::{ApiError, JobId, SourcePlan, SourceScope};

use crate::adapter::Result;

use super::local_io::LocalRootHandle;

const MAX_HELD_LOCAL_ROOTS: usize = 64;

#[derive(Debug, Clone, Default)]
pub struct LocalSourceAdapter {
    held_roots: Arc<Mutex<HashMap<JobId, Arc<LocalRootHandle>>>>,
    discovery_spools: Arc<Mutex<HashMap<JobId, Arc<tempfile::TempDir>>>>,
    contained_root: Option<(String, SourceScope, Arc<LocalRootHandle>)>,
}

impl LocalSourceAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_contained(
        source_root: &Path,
        scope: SourceScope,
        allowed_roots: &[PathBuf],
    ) -> Result<Self> {
        Ok(Self {
            held_roots: Arc::default(),
            discovery_spools: Arc::default(),
            contained_root: Some((
                source_root.to_string_lossy().into_owned(),
                scope,
                Arc::new(LocalRootHandle::from_allowed_roots(
                    source_root,
                    scope,
                    allowed_roots,
                )?),
            )),
        })
    }

    pub(super) fn root_for_discovery(&self, plan: &SourcePlan) -> Result<Arc<LocalRootHandle>> {
        if let Some((source, scope, handle)) = &self.contained_root {
            if source != &plan.request.source || *scope != plan.route.scope {
                return Err(root_state_error());
            }
            return Ok(Arc::clone(handle));
        }
        Ok(Arc::new(LocalRootHandle::for_source(
            Path::new(&plan.request.source),
            plan.route.scope,
        )?))
    }

    pub(super) fn retain_discovered_root(
        &self,
        job_id: JobId,
        handle: Arc<LocalRootHandle>,
        spool: Arc<tempfile::TempDir>,
    ) -> Result<()> {
        let mut held = self.held_roots.lock().map_err(poisoned_root_state)?;
        if !held.contains_key(&job_id) && held.len() >= MAX_HELD_LOCAL_ROOTS {
            return Err(ApiError::new(
                "adapter.local.root_state_capacity",
                axon_error::ErrorStage::Authorizing,
                "too many local source roots are awaiting acquisition",
            ));
        }
        held.insert(job_id, handle);
        self.discovery_spools
            .lock()
            .map_err(poisoned_root_state)?
            .insert(job_id, spool);
        Ok(())
    }

    pub(super) fn discovery_spool(&self, job_id: JobId) -> Result<Arc<tempfile::TempDir>> {
        self.discovery_spools
            .lock()
            .map_err(poisoned_root_state)?
            .get(&job_id)
            .cloned()
            .ok_or_else(root_state_error)
    }

    pub(super) fn held_root_for_acquisition(
        &self,
        plan: &SourcePlan,
    ) -> Result<Arc<LocalRootHandle>> {
        if let Some(handle) = self
            .held_roots
            .lock()
            .map_err(poisoned_root_state)?
            .get(&plan.job_id)
            .cloned()
        {
            return Ok(handle);
        }
        if self.contained_root.is_some() {
            return Err(root_state_error());
        }
        Ok(Arc::new(LocalRootHandle::for_source(
            Path::new(&plan.request.source),
            plan.route.scope,
        )?))
    }

    pub(super) fn release_root(&self, job_id: JobId) {
        if let Ok(mut held) = self.held_roots.lock() {
            held.remove(&job_id);
        }
        if let Ok(mut spools) = self.discovery_spools.lock() {
            spools.remove(&job_id);
        }
    }

    #[cfg(test)]
    pub(crate) fn held_root_count(&self) -> usize {
        self.held_roots.lock().map(|held| held.len()).unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn discovery_spool_count(&self) -> usize {
        self.discovery_spools
            .lock()
            .map(|spools| spools.len())
            .unwrap_or(0)
    }
}

fn poisoned_root_state<T>(_error: std::sync::PoisonError<T>) -> ApiError {
    root_state_error()
}

fn root_state_error() -> ApiError {
    ApiError::new(
        "adapter.local.root_state_failed",
        axon_error::ErrorStage::Authorizing,
        "local source root state is unavailable",
    )
}
