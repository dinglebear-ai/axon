use super::*;

#[test]
fn stage_json_names_are_snake_case_and_stable() {
    let cases = [
        (ErrorStage::Parsing, "parsing"),
        (ErrorStage::Validation, "validation"),
        (ErrorStage::Resolving, "resolving"),
        (ErrorStage::Routing, "routing"),
        (ErrorStage::Authorizing, "authorizing"),
        (ErrorStage::Planning, "planning"),
        (ErrorStage::Leasing, "leasing"),
        (ErrorStage::Discovering, "discovering"),
        (ErrorStage::Diffing, "diffing"),
        (ErrorStage::Fetching, "fetching"),
        (ErrorStage::Rendering, "rendering"),
        (ErrorStage::Normalizing, "normalizing"),
        (ErrorStage::ParsingContent, "parsing_content"),
        (ErrorStage::Graphing, "graphing"),
        (ErrorStage::Preparing, "preparing"),
        (ErrorStage::Embedding, "embedding"),
        (ErrorStage::Upserting, "upserting"),
        (ErrorStage::Publishing, "publishing"),
        (ErrorStage::Cleaning, "cleaning"),
        (ErrorStage::Retrieving, "retrieving"),
        (ErrorStage::Synthesizing, "synthesizing"),
        (ErrorStage::Observing, "observing"),
    ];
    assert_eq!(cases.len(), 22, "expected 22 stage variants covered");
    for (stage, name) in cases {
        assert_eq!(serde_json::to_value(stage).unwrap(), name);
        let back: ErrorStage = serde_json::from_value(serde_json::json!(name)).unwrap();
        assert_eq!(back, stage);
    }
}
