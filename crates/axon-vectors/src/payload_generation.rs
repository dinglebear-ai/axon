use axon_api::source::{ApiError, SourceGenerationId};

use crate::payload::VectorPayloadValidationError;

impl From<VectorPayloadValidationError> for ApiError {
    fn from(error: VectorPayloadValidationError) -> Self {
        ApiError::new(
            "vector.invalid_generation",
            axon_error::ErrorStage::Preparing,
            error.to_string(),
        )
    }
}

pub fn generation_payload_i64(
    generation: &SourceGenerationId,
    field: &str,
) -> Result<i64, VectorPayloadValidationError> {
    let raw = generation.0.trim();
    let numeric = raw
        .strip_prefix("gen_")
        .unwrap_or(raw)
        .rsplit_once('_')
        .map_or_else(|| raw.strip_prefix("gen_").unwrap_or(raw), |(_, tail)| tail);
    let value =
        numeric
            .parse::<i64>()
            .map_err(|_| VectorPayloadValidationError::InvalidGeneration {
                field: field.to_string(),
            })?;
    if value < 0 {
        return Err(VectorPayloadValidationError::InvalidGeneration {
            field: field.to_string(),
        });
    }
    Ok(value)
}
