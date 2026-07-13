use super::*;

#[test]
fn test_matches_shopify_product_url() {
    assert!(matches(
        "https://mystore.myshopify.com/products/awesome-widget"
    ));
    assert!(matches("https://shop.example.com/products/blue-shirt"));
    // Non-Shopify blocked hosts
    assert!(!matches("https://github.com/products/feature"));
    assert!(!matches("https://www.amazon.com/products/item"));
    // No handle
    assert!(!matches("https://mystore.myshopify.com/products/"));
}

#[test]
fn test_build_extra_fields() {
    let extra = build_extra("shop.example.com", "Acme Corp", "Widgets", "awesome-widget");
    assert_eq!(extra["shop_host"], "shop.example.com");
    assert_eq!(extra["shop_vendor"], "Acme Corp");
    assert_eq!(extra["shop_product_type"], "Widgets");
    assert_eq!(extra["shop_handle"], "awesome-widget");

    // Empty optional fields should not appear
    let extra_minimal = build_extra("shop.example.com", "", "", "");
    assert!(extra_minimal.get("shop_vendor").is_none());
    assert!(extra_minimal.get("shop_product_type").is_none());
    assert!(extra_minimal.get("shop_handle").is_none());
    assert_eq!(extra_minimal["shop_host"], "shop.example.com");
}
