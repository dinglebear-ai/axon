#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExecutionMode {
    Enqueued,
    InProcess,
    Synchronous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StartDisposition {
    Enqueued,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JobStartOutcome<T> {
    pub disposition: StartDisposition,
    pub execution_mode: ExecutionMode,
    pub result: T,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JobEnqueued {
    pub job_id: String,
    pub kind: String,
    pub execution_mode: ExecutionMode,
    pub output_dir: Option<String>,
    pub predicted_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ServiceError {
    UnsupportedInCurrentMode {
        feature: &'static str,
        reason: String,
    },
    MissingDependency {
        dependency: &'static str,
        reason: String,
    },
    BackendUnavailable {
        backend: &'static str,
        reason: String,
    },
    ValidationFailed(String),
    NotFound {
        resource: &'static str,
        id: String,
    },
    Conflict(String),
    Internal(String),
}
