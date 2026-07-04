//! Shared metric sample shapes for the target observability boundary.

pub const MODULE_NAME: &str = "metric";

use axon_api::source::{MetadataMap, Timestamp};

#[derive(Debug, Clone, PartialEq)]
pub struct MetricSample {
    pub name: String,
    pub value: f64,
    pub unit: Option<String>,
    pub labels: MetadataMap,
    pub timestamp: Timestamp,
}
