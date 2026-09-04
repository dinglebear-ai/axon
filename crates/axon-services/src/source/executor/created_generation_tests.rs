use super::*;

fn cleanup_error() -> ApiError {
    ApiError::new(
        "ledger.cleanup_debt_write_failed",
        ErrorStage::Cleaning,
        "cleanup debt write failed",
    )
}

#[test]
fn cleanup_handoff_failure_turns_a_successful_pipeline_into_an_error() {
    let error = merge_generation_cleanup(Ok(7_u8), Err(cleanup_error())).unwrap_err();

    assert!(error.to_string().contains("cleanup debt write failed"));
}

#[test]
fn cleanup_handoff_failure_preserves_primary_pipeline_error() {
    let error = merge_generation_cleanup::<u8>(
        Err(anyhow::anyhow!("primary pipeline failure")),
        Err(cleanup_error()),
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("artifact cleanup handoff also failed")
    );
    assert!(format!("{error:#}").contains("primary pipeline failure"));
}
