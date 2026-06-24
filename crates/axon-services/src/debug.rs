use crate::types::DebugResult;
use axon_core::config::Config;
use axon_core::health::build_doctor_report;
use axon_core::llm::{self, CompletionRequest};
use std::error::Error;

#[must_use = "debug_report returns a Result that should be handled"]
pub async fn debug_report(cfg: &Config, user_context: &str) -> Result<DebugResult, Box<dyn Error>> {
    let pending_jobs = axon_jobs::store::count_pending_jobs(&cfg.sqlite_path).await;
    let doctor_report = build_doctor_report(cfg, pending_jobs).await?;

    let prompt = format!(
        "Analyze this Axon doctor report and provide actionable troubleshooting guidance.\n\
         Prioritize root causes and concrete fix commands.\n\
         Keep it concise and operator-friendly.\n\
         Include:\n\
         1) likely root causes ordered by confidence\n\
         2) exact verification commands\n\
         3) exact remediation commands\n\
         4) what to check next if fixes fail\n\n\
         Optional operator context:\n{}\n\n\
         Doctor report JSON:\n{}",
        if user_context.is_empty() {
            "(none)"
        } else {
            user_context
        },
        serde_json::to_string_pretty(&doctor_report)?
    );

    let mut request = CompletionRequest::new(prompt).system_prompt(
        "You are a senior self-hosted infrastructure debugging assistant. Be precise and avoid generic advice."
    );
    request = request.backend_from_config(cfg);
    let model = llm::configured_model_from_config(cfg);
    if let Some(model) = model.clone() {
        request = request.model(model);
    }
    let completion = llm::complete_text(request)
        .await
        .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
    let analysis = if completion.text.trim().is_empty() {
        "(no debug response)"
    } else {
        completion.text.as_str()
    };

    Ok(DebugResult {
        payload: serde_json::json!({
            "doctor_report": doctor_report,
            "llm_debug": {
                "model": model.map_or(serde_json::Value::Null, |model| serde_json::json!(model)),
                "analysis": analysis,
            }
        }),
    })
}
