use super::*;

impl FakeJobWatchStore {
    /// Test-only inspection seam that bypasses public event visibility rules.
    pub async fn recorded_events(&self, job_id: JobId) -> Vec<JobEvent> {
        self.state
            .lock()
            .await
            .events
            .get(&job_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Test-only history of authoritative status snapshots.
    pub async fn recorded_status_updates(&self, job_id: JobId) -> Vec<JobStatusUpdate> {
        self.state
            .lock()
            .await
            .status_updates
            .get(&job_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Test-only history of liveness/provider reservation heartbeats.
    pub async fn recorded_heartbeats(&self, job_id: JobId) -> Vec<JobHeartbeat> {
        self.state
            .lock()
            .await
            .heartbeats
            .get(&job_id)
            .cloned()
            .unwrap_or_default()
    }
}
