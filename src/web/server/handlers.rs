#[path = "handlers/admin.rs"]
pub mod admin;
#[path = "handlers/artifacts.rs"]
pub mod artifacts;
#[path = "handlers/ask.rs"]
pub mod ask;
#[path = "handlers/ask_stream.rs"]
pub mod ask_stream;
#[path = "handlers/async_jobs.rs"]
pub mod async_jobs;
#[path = "handlers/auth.rs"]
pub mod auth;
#[path = "handlers/chat.rs"]
pub mod chat;
#[path = "handlers/chat_stream.rs"]
pub mod chat_stream;
#[path = "handlers/config.rs"]
pub mod config;
#[path = "handlers/discovery.rs"]
pub mod discovery;
#[path = "handlers/exploration.rs"]
pub mod exploration;
#[path = "handlers/jobs.rs"]
pub mod jobs;
#[path = "handlers/memory.rs"]
pub mod memory;
#[path = "handlers/mobile_sessions.rs"]
pub mod mobile_sessions;
#[path = "handlers/rag.rs"]
pub mod rag;
#[path = "handlers/rest.rs"]
pub(crate) mod rest;
#[path = "handlers/setup.rs"]
pub mod setup;

pub use ask::v1_ask;
pub use ask_stream::v1_ask_stream;
pub use auth::{login, panel_state};
pub use chat::v1_chat;
pub use chat_stream::v1_chat_stream;
pub use config::{
    collections, get_config, get_env_config, ops, panel_artifact, panel_collections, panel_command,
    panel_doctor, panel_status, save_config, save_env_config,
};
pub use setup::setup_targets;
