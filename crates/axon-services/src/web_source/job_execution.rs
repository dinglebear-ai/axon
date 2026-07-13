use axon_api::source::JobId;

#[derive(Debug, Clone)]
pub(crate) struct WebSourceJobExecution {
    pub(crate) job_id: JobId,
    pub(crate) owns_status: bool,
}
