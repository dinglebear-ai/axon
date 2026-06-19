use axon::services::types::{RestRouteAuth, rest_route_inventory};
use serde::Serialize;

#[derive(Serialize)]
struct AndroidRouteContract {
    method: &'static str,
    path: &'static str,
    #[serde(rename = "requiresAuth")]
    requires_auth: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let contracts = rest_route_inventory()
        .iter()
        .filter(|route| route.openapi)
        .map(|route| AndroidRouteContract {
            method: route.method,
            path: route.path,
            requires_auth: route.auth != RestRouteAuth::Public,
        })
        .collect::<Vec<_>>();

    println!("{}", serde_json::to_string_pretty(&contracts)?);
    Ok(())
}
