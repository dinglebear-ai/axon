use crate::cli::commands::CommandFuture;
use crate::core::config::Config;
use crate::core::ui::muted;
use crate::services::context::ServiceContext;

pub fn run_sync<'a>(cfg: &'a Config, _ctx: &'a ServiceContext) -> CommandFuture<'a> {
    Box::pin(async move {
        let subcommand = cfg
            .positional
            .first()
            .map(String::as_str)
            .unwrap_or("pending");
        if subcommand != "pending" {
            return Err(format!("unknown sync subcommand: {subcommand}").into());
        }
        if cfg.json_output {
            println!("{}", serde_json::json!({ "synced": 0, "pending": 0 }));
        } else {
            println!("{}", muted("Sync pending: 0 synced, 0 pending"));
        }
        Ok(())
    })
}
