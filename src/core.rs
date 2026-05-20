pub mod config;
pub mod content;
pub mod endpoints;
pub mod health;
pub mod http;
pub mod logging;
pub mod paths;
pub mod structured;
pub mod ui;

#[cfg(test)]
#[path = "core/endpoints_tests.rs"]
mod endpoints_tests;
