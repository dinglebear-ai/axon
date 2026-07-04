//! Runtime event registry used by schema-contract generation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSpec {
    pub name: &'static str,
    pub phase: &'static str,
    pub status: &'static str,
}

pub fn event_registry() -> &'static [EventSpec] {
    &[
        EventSpec {
            name: "SourceProgressEvent",
            phase: "source",
            status: "running",
        },
        EventSpec {
            name: "JobEvent",
            phase: "job",
            status: "running",
        },
        EventSpec {
            name: "JobHeartbeat",
            phase: "job",
            status: "running",
        },
        EventSpec {
            name: "ProviderReservationEvent",
            phase: "provider",
            status: "running",
        },
    ]
}
