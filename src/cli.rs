pub mod client;
pub mod commands;
pub mod rest_client;
pub mod route;
pub mod server_mode;

#[cfg(test)]
#[path = "cli/rest_client_tests.rs"]
mod rest_client_tests;
#[cfg(test)]
#[path = "cli/route_tests.rs"]
mod route_tests;
