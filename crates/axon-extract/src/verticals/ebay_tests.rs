use super::*;

#[test]
fn matches_itm_path() {
    assert!(matches("https://www.ebay.com/itm/123456789"));
    assert!(matches("https://ebay.com/itm/123456789"));
}

#[test]
fn matches_sch_path() {
    assert!(matches("https://www.ebay.com/sch/i.html?_nkw=gpu"));
}

#[test]
fn rejects_non_ebay() {
    assert!(!matches("https://example.com/itm/123456789"));
}

#[test]
fn extract_item_id_itm() {
    let id = extract_item_id("https://www.ebay.com/itm/123456789");
    assert_eq!(id.as_deref(), Some("123456789"));
}

#[test]
fn build_extra_ebay_with_jsonld() {
    let jsonld = serde_json::json!({
        "brand": { "name": "Sony" },
        "offers": {
            "price": "199.99",
            "itemCondition": "https://schema.org/NewCondition",
            "availability": "https://schema.org/InStock"
        },
        "aggregateRating": { "ratingValue": 4.2, "reviewCount": 50 }
    });
    let extra = build_extra(Some(&jsonld), Some("123456789"));
    assert_eq!(extra["ebay_brand"], "Sony");
    assert_eq!(extra["ebay_price"], "199.99");
    assert_eq!(extra["ebay_condition"], "New");
    assert_eq!(extra["ebay_availability"], "InStock");
    assert_eq!(extra["ebay_rating"], 4.2);
    assert_eq!(extra["ebay_review_count"], 50u64);
    assert_eq!(extra["ebay_item_id"], "123456789");
}

#[test]
fn build_extra_ebay_no_jsonld() {
    let extra = build_extra(None, Some("987654321"));
    assert_eq!(extra["ebay_item_id"], "987654321");
    assert!(extra.get("ebay_brand").is_none());
}

#[test]
fn build_extra_ebay_no_item_id() {
    let extra = build_extra(None, None);
    assert!(extra.get("ebay_item_id").is_none());
}
