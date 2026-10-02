use super::*;
use crate::boundary::FakeAdapterProviders;

struct GoneFetch(u16);
#[async_trait::async_trait]
impl FetchProvider for GoneFetch {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource> {
        let mut fetched = FakeAdapterProviders::new().with_fetch_text(
            "This error page must never replace indexed documentation or become searchable content."
        ).fetch(request).await?;
        fetched.status = self.0;
        Ok(fetched)
    }
    async fn capabilities(&self) -> Result<ProviderCapability> {
        FetchProvider::capabilities(&FakeAdapterProviders::new()).await
    }
}

#[tokio::test]
async fn missing_http_page_cannot_index_the_error_response_body() {
    let item = ManifestItem {
        source_id: SourceId::new("source"),
        source_item_key: SourceItemKey::new("gone"),
        canonical_uri: "https://example.test/docs/gone".into(),
        item_kind: ItemKind::WebPage,
        content_kind: Some(ContentKind::Markdown),
        display_path: None,
        parent_key: None,
        size_bytes: None,
        content_hash: None,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: MetadataMap::new(),
        graph_hints: vec![],
    };
    for status in [404, 410] {
        let acquired = acquire_via_fetch(&GoneFetch(status), &item, CachePolicy::Bypass, &[])
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(acquired.content_ref, ContentRef::InlineText { ref text } if text.is_empty()),
            "confirmed missing pages must carry no body into normalization/preparation"
        );
        assert_eq!(
            acquired.metadata.get("web_status").and_then(Value::as_u64),
            Some(status.into())
        );
    }
}
