use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            JsonSchema,
            utoipa::ToSchema,
        )]
        #[schema(value_type = String)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }
    };
}

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            JsonSchema,
            utoipa::ToSchema,
        )]
        #[schema(value_type = String, format = Uuid)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new(value: Uuid) -> Self {
                Self(value)
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }
    };
}

uuid_id!(JobId);
uuid_id!(StageId);
uuid_id!(BatchId);

string_id!(SourceId);
string_id!(SourceItemKey);
string_id!(SourceGenerationId);
string_id!(DocumentId);
string_id!(ChunkId);
string_id!(VectorPointId);
string_id!(ProviderId);
string_id!(ArtifactId);
string_id!(CleanupDebtId);
string_id!(LeaseId);
string_id!(WatchId);
string_id!(MemoryId);
string_id!(GraphNodeId);
string_id!(GraphEdgeId);
string_id!(ConfigSnapshotId);

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    utoipa::ToSchema,
)]
#[serde(transparent)]
#[schema(value_type = String, format = DateTime)]
pub struct Timestamp(pub String);

impl From<DateTime<Utc>> for Timestamp {
    fn from(value: DateTime<Utc>) -> Self {
        Self(value.to_rfc3339())
    }
}

#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(transparent)]
#[schema(value_type = Object)]
pub struct MetadataMap(pub BTreeMap<String, serde_json::Value>);

impl MetadataMap {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Deref for MetadataMap {
    type Target = BTreeMap<String, serde_json::Value>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MetadataMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
