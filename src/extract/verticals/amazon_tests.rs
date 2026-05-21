use super::*;

#[test]
fn matches_dp_path() {
    assert!(matches("https://www.amazon.com/dp/B08N5WRWNW"));
    assert!(matches("https://amazon.com/dp/B08N5WRWNW"));
}

#[test]
fn matches_gp_product_path() {
    assert!(matches("https://www.amazon.com/gp/product/B08N5WRWNW"));
}

#[test]
fn rejects_non_amazon() {
    assert!(!matches("https://example.com/dp/B08N5WRWNW"));
}

#[test]
fn extract_asin_dp() {
    let asin = extract_asin("https://www.amazon.com/dp/B08N5WRWNW");
    assert_eq!(asin.as_deref(), Some("B08N5WRWNW"));
}

#[test]
fn build_extra_amazon_with_jsonld() {
    let jsonld = serde_json::json!({
        "brand": { "name": "Acme" },
        "offers": { "price": "29.99", "priceCurrency": "USD", "availability": "https://schema.org/InStock" },
        "aggregateRating": { "ratingValue": 4.5, "reviewCount": 100 }
    });
    let extra = build_extra(Some(&jsonld), Some("B08N5WRWNW"));
    assert_eq!(extra["amz_brand"], "Acme");
    assert_eq!(extra["amz_price"], "29.99");
    assert_eq!(extra["amz_currency"], "USD");
    assert_eq!(extra["amz_availability"], "InStock");
    assert_eq!(extra["amz_rating"], 4.5);
    assert_eq!(extra["amz_review_count"], 100u64);
    assert_eq!(extra["amz_asin"], "B08N5WRWNW");
}

#[test]
fn build_extra_amazon_no_jsonld() {
    let extra = build_extra(None, Some("B0ASIN123"));
    assert_eq!(extra["amz_asin"], "B0ASIN123");
    assert!(extra.get("amz_brand").is_none());
}

#[test]
fn build_extra_amazon_no_asin() {
    let extra = build_extra(None, None);
    assert!(extra.get("amz_asin").is_none());
}
