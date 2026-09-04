use super::{
    build_extract_web_config, should_try_vertical_extractor, vertical_doc_to_extract_run,
    write_extract_summary, write_item_lines,
};
use axon_core::config::Config;
use axon_extract::ScrapedDoc;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

use super::ExtractProgress;

#[derive(Default)]
struct CountingAsyncWriter {
    writes: usize,
    flushes: usize,
    bytes: usize,
}

#[derive(Default)]
struct YieldOnceWriter {
    pending_next: bool,
    pending_polls: usize,
    bytes: usize,
}

impl AsyncWrite for YieldOnceWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if !self.pending_next {
            self.pending_next = true;
            self.pending_polls += 1;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        self.pending_next = false;
        self.bytes += buf.len();
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.poll_flush(cx)
    }
}

impl AsyncWrite for CountingAsyncWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.writes += 1;
        self.bytes += buf.len();
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.flushes += 1;
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.poll_flush(cx)
    }
}

#[tokio::test]
async fn extraction_item_writes_are_buffered_and_fully_flushed() {
    let lines = (0..1_000).map(|_| "{}\n".to_string()).collect::<Vec<_>>();
    let mut writer = BufWriter::with_capacity(8 * 1024, CountingAsyncWriter::default());

    write_item_lines(&mut writer, &lines).await.unwrap();
    assert_eq!(
        writer.get_ref().writes,
        0,
        "small writes should remain buffered"
    );
    writer.flush().await.unwrap();

    assert_eq!(writer.get_ref().bytes, 3_000);
    assert_eq!(writer.get_ref().writes, 1, "buffer must collapse syscalls");
    assert_eq!(
        writer.get_ref().flushes,
        1,
        "no pending bytes at completion"
    );
}

#[tokio::test]
async fn extraction_writes_cooperate_when_the_async_sink_applies_backpressure() {
    let lines = vec![format!("{}\n", "x".repeat(16 * 1024)); 4];
    let mut writer = BufWriter::with_capacity(8 * 1024, YieldOnceWriter::default());

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        write_item_lines(&mut writer, &lines),
    )
    .await
    .expect("async writes must yield and resume under sink backpressure")
    .expect("write lines");
    writer.flush().await.expect("flush pending bytes");

    assert_eq!(writer.get_ref().bytes, 4 * (16 * 1024 + 1));
    assert!(
        writer.get_ref().pending_polls >= 4,
        "the async writer must observe and resume from Pending"
    );
}

#[test]
fn completed_extract_run_advances_urls_and_items() {
    let prior = ExtractProgress::new(3);
    let next = prior.completed_url("https://example.com/a", 7);
    assert_eq!(next.urls_total(), 3);
    assert_eq!(next.urls_done(), 1);
    assert_eq!(next.items_done(), 7);
    assert_eq!(next.last_completed_url(), Some("https://example.com/a"));
}

#[test]
fn extract_progress_is_monotonic_across_out_of_order_completion() {
    let progress = ExtractProgress::new(2)
        .completed_url("https://example.com/b", 3)
        .completed_url("https://example.com/a", 1);
    assert_eq!(progress.urls_done(), 2);
    assert_eq!(progress.items_done(), 4);
}

#[test]
fn extract_progress_ignores_completions_after_total_is_reached() {
    let progress = ExtractProgress::new(1)
        .completed_url("https://example.com/a", 2)
        .completed_url("https://example.com/b", 99);
    assert_eq!(progress.urls_done(), 1);
    assert_eq!(progress.items_done(), 2);
    assert_eq!(progress.last_completed_url(), Some("https://example.com/a"));
}

#[tokio::test]
async fn extract_summary_redacts_secrets_before_writing() {
    let output_root = tempfile::tempdir().expect("output root");
    let cfg = Config {
        output_dir: output_root.path().to_path_buf(),
        output_path: None,
        ..Config::default()
    };

    let path = write_extract_summary(
        &cfg,
        &serde_json::json!({
            "runs": [{"error": "Authorization: Bearer abcdef0123456789abcdef"}],
        }),
    )
    .await
    .expect("write summary");

    let written = std::fs::read_to_string(&path).expect("read summary");
    assert!(!written.contains("abcdef0123456789abcdef"));
}

#[tokio::test]
async fn extract_summary_preserves_explicit_output_outside_output_dir() {
    let output_root = tempfile::tempdir().expect("output root");
    let explicit_root = tempfile::tempdir().expect("explicit root");
    let explicit = explicit_root.path().join("summary.json");
    let cfg = Config {
        output_dir: output_root.path().to_path_buf(),
        output_path: Some(explicit.clone()),
        ..Config::default()
    };

    let path = write_extract_summary(&cfg, &serde_json::json!({"ok": true}))
        .await
        .expect("write summary");

    assert_eq!(path, explicit);
    assert!(explicit.exists());
    assert!(!output_root.path().join("extract-summary.json").exists());
}

#[tokio::test]
async fn extract_summary_defaults_to_managed_output_dir() {
    let output_root = tempfile::tempdir().expect("output root");
    let cfg = Config {
        output_dir: output_root.path().to_path_buf(),
        output_path: None,
        ..Config::default()
    };

    let path = write_extract_summary(&cfg, &serde_json::json!({"ok": true}))
        .await
        .expect("write summary");

    assert_eq!(path, output_root.path().join("extract-summary.json"));
    assert!(path.exists());
}

#[test]
fn vertical_doc_becomes_extract_item() {
    let run = vertical_doc_to_extract_run(ScrapedDoc {
        url: "https://pypi.org/project/requests/".to_string(),
        markdown: "# requests\n\nPython HTTP library".to_string(),
        title: Some("requests".to_string()),
        extractor_name: "pypi",
        extractor_version: 3,
        structured: Some(serde_json::json!({"name": "requests"})),
        follow_crawl_urls: vec!["https://requests.readthedocs.io/".to_string()],
        extra: Some(serde_json::json!({"pkg_name": "requests"})),
    });

    assert_eq!(run.pages_visited, 1);
    assert_eq!(run.pages_with_data, 1);
    assert_eq!(run.results.len(), 1);
    assert_eq!(run.parser_hits.get("vertical:pypi"), Some(&1));
    assert_eq!(run.results[0]["extractor_name"], "pypi");
    assert_eq!(run.results[0]["extra"]["pkg_name"], "requests");
    assert_eq!(run.results[0]["structured"]["name"], "requests");
}

#[test]
fn prompt_driven_extract_skips_vertical_shortcut() {
    let cfg = Config {
        enable_verticals: true,
        ..Config::default()
    };
    let prompted = build_extract_web_config(
        &cfg,
        "https://pypi.org/project/requests/".to_string(),
        "extract license metadata",
    );
    let unprompted = build_extract_web_config(
        &cfg,
        "https://pypi.org/project/requests/".to_string(),
        "   ",
    );

    assert!(!should_try_vertical_extractor(&cfg, &prompted));
    assert!(should_try_vertical_extractor(&cfg, &unprompted));
}
