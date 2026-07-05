//! Vector payload registry used by schema-contract generation.

pub fn vector_payload_required_fields() -> &'static [&'static str] {
    crate::payload::VECTOR_REQUIRED_FIELDS
}

pub fn vector_payload_source_families() -> &'static [&'static str] {
    crate::payload::VECTOR_SOURCE_FAMILIES
}
