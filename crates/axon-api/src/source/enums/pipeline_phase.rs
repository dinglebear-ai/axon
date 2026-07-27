use super::PipelinePhase;

impl PipelinePhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Requested => "requested",
            Self::Resolving => "resolving",
            Self::Routing => "routing",
            Self::Authorizing => "authorizing",
            Self::Planning => "planning",
            Self::Leasing => "leasing",
            Self::Discovering => "discovering",
            Self::Diffing => "diffing",
            Self::Fetching => "fetching",
            Self::Rendering => "rendering",
            Self::Enriching => "enriching",
            Self::Normalizing => "normalizing",
            Self::Parsing => "parsing",
            Self::Graphing => "graphing",
            Self::Preparing => "preparing",
            Self::Batching => "batching",
            Self::Embedding => "embedding",
            Self::Vectorizing => "vectorizing",
            Self::Upserting => "upserting",
            Self::Retrieving => "retrieving",
            Self::Synthesizing => "synthesizing",
            Self::Evaluating => "evaluating",
            Self::Publishing => "publishing",
            Self::Cleaning => "cleaning",
            Self::Complete => "complete",
            Self::Canceled => "canceled",
        }
    }
}
