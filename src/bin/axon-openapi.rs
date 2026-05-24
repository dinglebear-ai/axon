fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = axon::web::openapi_document();
    println!("{}", serde_json::to_string_pretty(&document)?);
    Ok(())
}
