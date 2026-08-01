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

    pub const fn default_successors(self) -> &'static [Self] {
        match self {
            Self::Queued => &[
                Self::Requested,
                Self::Resolving,
                Self::Planning,
                Self::Canceled,
            ],
            Self::Requested => &[Self::Resolving, Self::Planning, Self::Canceled],
            Self::Resolving => &[Self::Routing, Self::Canceled],
            Self::Routing => &[Self::Authorizing, Self::Planning, Self::Canceled],
            Self::Authorizing => &[Self::Planning, Self::Leasing, Self::Canceled],
            Self::Planning => &[
                Self::Leasing,
                Self::Discovering,
                Self::Retrieving,
                Self::Canceled,
            ],
            Self::Leasing => &[Self::Discovering, Self::Fetching, Self::Canceled],
            Self::Discovering => &[
                Self::Diffing,
                Self::Fetching,
                Self::Publishing,
                Self::Canceled,
            ],
            Self::Diffing => &[
                Self::Fetching,
                Self::Publishing,
                Self::Cleaning,
                Self::Canceled,
            ],
            Self::Fetching => &[
                Self::Rendering,
                Self::Enriching,
                Self::Normalizing,
                Self::Canceled,
            ],
            Self::Rendering => &[Self::Enriching, Self::Normalizing, Self::Canceled],
            Self::Enriching => &[Self::Normalizing, Self::Parsing, Self::Canceled],
            Self::Normalizing => &[Self::Parsing, Self::Preparing, Self::Canceled],
            Self::Parsing => &[Self::Graphing, Self::Preparing, Self::Canceled],
            Self::Graphing => &[Self::Preparing, Self::Publishing, Self::Canceled],
            Self::Preparing => &[
                Self::Batching,
                Self::Embedding,
                Self::Publishing,
                Self::Canceled,
            ],
            Self::Batching => &[Self::Embedding, Self::Vectorizing, Self::Canceled],
            Self::Embedding => &[Self::Vectorizing, Self::Upserting, Self::Canceled],
            Self::Vectorizing => &[Self::Upserting, Self::Publishing, Self::Canceled],
            Self::Upserting => &[Self::Publishing, Self::Canceled],
            Self::Retrieving => &[
                Self::Synthesizing,
                Self::Evaluating,
                Self::Complete,
                Self::Canceled,
            ],
            Self::Synthesizing => &[Self::Evaluating, Self::Complete, Self::Canceled],
            Self::Evaluating => &[Self::Publishing, Self::Complete, Self::Canceled],
            Self::Publishing => &[
                Self::Graphing,
                Self::Cleaning,
                Self::Complete,
                Self::Canceled,
            ],
            Self::Cleaning => &[Self::Complete, Self::Canceled],
            Self::Complete | Self::Canceled => &[],
        }
    }
}
