# Issue 174 Artifact Results Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Make user-facing Axon web panel and palette surfaces render screenshot/artifact previews from `ArtifactHandle.relative_path` without exposing unsafe filesystem paths or unauthenticated image URLs.

**Architecture:** Keep automation REST job submission unchanged and fix the artifact bridge between service output and app presentation. Backend artifact serving becomes one shared, streaming, auth-protected implementation; web panel uses panel-auth artifact URLs; the Tauri palette uses a dedicated capped artifact bridge command that returns preview bytes for safe raster image artifacts.

**Tech Stack:** Rust, Axum, Tokio fs, `tokio_util::io::ReaderStream`, serde/utoipa, reqwest/Tauri commands, React, TypeScript, Vitest, Testing Library.

---

## Engineering Review Decisions

This plan was revised after Lavra engineering review.

- **Defer generalized app job polling.** It is useful UX work, but it is not required to fix screenshot/artifact rendering and adds lifecycle/polling risk. Track it as separate follow-up bead `axon_rust-gnb6.5`.
- **Artifact serving must stream files.** Do not broaden `/v1/artifacts` and `/api/panel/artifact` usage while buffering whole files with `tokio::fs::read`.
- **Only raster images can render inline.** HTML, SVG, and unknown artifacts must not execute under the Axon origin. JSON, markdown, text, and logs keep accurate passive content types but are still served as attachments with `X-Content-Type-Options: nosniff`.
- **Symlink rejection must happen before canonicalization.** Reject symlink components in the requested path, not only the final canonical target.
- **The palette needs an explicit artifact bridge.** Existing `axon_http_request` rejects query-string paths and returns parsed text/JSON, not bytes. Add a narrow `axon_artifact_request(relative_path)` command with validation, route construction, content-type allowlist, and byte cap.
- **Absolute server paths are debug metadata only.** Screenshot UI must prefer `artifact_handle.display_path` and `artifact_handle.relative_path`; do not render absolute `path` as a primary result or `file://` preview source.
- **Docs are scoped to artifact preview/download.** Do not document generalized terminal job polling until that work ships.

## Current-State Review

Issue 174 is still open, but the original `apps/desktop` paths are stale. The active desktop-style app lives under `apps/palette-tauri`.

Already present:

- `src/web/server/handlers/artifacts.rs` implements `GET /v1/artifacts?path=...` plus legacy path form.
- `src/web/server/routing.rs` protects `/v1/artifacts` with `axon:read`.
- `src/web/server/handlers/config.rs` has `/api/panel/artifact/{*path}` for panel-auth artifact reads.
- `apps/web/app/command-format.ts` extracts artifact handles and creates `imageUrl`.
- `apps/palette-tauri/src/components/palette/OperationResultView.tsx` has a screenshot view.
- `apps/palette-tauri/src/lib/axonClient.ts` routes `screenshot` to `POST /v1/screenshot`.

Completed implementation status:

- The web panel now builds screenshot image URLs through the panel-auth artifact route.
- The panel artifact route now uses the shared artifact-serving implementation.
- Artifact serving streams files, rejects unsafe paths and symlink components, serves active/ambiguous content as opaque attachments, and serves passive text-like content as typed attachments with `nosniff`.
- The Tauri palette now loads canonical raster artifact bytes through a dedicated capped bridge.
- Palette screenshot output now prefers artifact display metadata and keeps absolute server paths out of primary UI.

Remaining scope:

- Generalized user-facing app job polling/waited terminal job results remain deferred follow-up work in `axon_rust-gnb6.5`, tracked outside this artifact-preview implementation.

## File Structure

- Modify: `src/web/server/handlers/artifacts.rs` - shared safe artifact resolution, symlink-component rejection, streaming response body, safe content headers.
- Modify: `src/web/server/handlers/artifacts_tests.rs` - structural path, symlink, content-type/header, and missing-root tests.
- Modify: `src/web/server/handlers/config.rs` - route `/api/panel/artifact/{*path}` through shared artifact serving after panel auth.
- Test: `src/web/server/handlers/rest_auth_tests.rs` or existing panel handler tests - assert panel artifact route requires panel auth and serves raster bytes with content type.
- Modify: `apps/web/app/command-format.ts` - export a segment-encoding `panelArtifactUrl()` helper and raster-artifact predicate.
- Modify: `apps/web/app/panel-components.tsx` - use the shared helper for preview image `src`, artifact links, and download/open links.
- Test: create `apps/web/app/command-format.test.ts` - cover screenshot image URL, path encoding, non-image artifact handling, and raw-result suppression.
- Modify: `apps/palette-tauri/src-tauri/src/axon_bridge.rs` - add dedicated `axon_artifact_request(relative_path)` command with validation, byte cap, content-type allowlist, and base64 response.
- Test: `apps/palette-tauri/src-tauri/src/axon_bridge_tests.rs` or equivalent - cover artifact command validation, query construction, disallowed paths, disallowed content types, and byte cap behavior.
- Create: `apps/palette-tauri/src/lib/artifactPreview.ts` - call the dedicated artifact bridge command, convert base64 to an object URL, and expose a typed loader.
- Modify: `apps/palette-tauri/src/components/palette/OperationResultView.tsx` - render screenshot artifact handles through the loader, with visible failed-preview state and robust object URL cleanup.
- Test: `apps/palette-tauri/src/components/palette/OperationResultView.test.tsx` - assert screenshot payloads with only `artifact_handle.relative_path` render an image, stale promises are cleaned up, and absolute server paths are not displayed.
- Modify: `apps/palette-tauri/src/lib/format.ts` - stop making raw server paths the primary screenshot text output.
- Test: `apps/palette-tauri/src/lib/format.test.ts` - assert screenshot formatting prefers artifact/display metadata and omits absolute `path:`.
- Modify: `docs/reference/job-lifecycle.md` and `docs/reference/http-api.md` - document artifact result contract and route auth expectations only.

## Task 1: Share Streaming Safe Artifact Serving Between REST And Panel

**Files:**
- Modify: `src/web/server/handlers/artifacts.rs`
- Modify: `src/web/server/handlers/config.rs`
- Test: `src/web/server/handlers/artifacts_tests.rs`
- Test: `src/web/server/handlers/rest_auth_tests.rs`

- [x] **Step 1: Write failing shared-helper tests**

Add tests in `src/web/server/handlers/artifacts_tests.rs`:

```rust
#[test]
fn unsafe_artifact_paths_are_rejected_structurally() {
    assert!(is_structurally_unsafe("../secret.txt"));
    assert!(is_structurally_unsafe("screenshots/../secret.txt"));
    assert!(is_structurally_unsafe("screenshots/%2e%2e/secret.txt"));
    assert!(is_structurally_unsafe(r"screenshots\\..\\secret.txt"));
    assert!(is_structurally_unsafe(r"C:\\Windows\\secret.txt"));
    assert!(is_structurally_unsafe("screenshots/shot.png\0"));
}

#[test]
fn raster_images_are_inline_but_active_content_is_attachment() {
    assert_eq!(artifact_headers_for_path("screenshots/shot.png").content_type, "image/png");
    assert!(artifact_headers_for_path("screenshots/shot.png").content_disposition.is_none());
    assert_eq!(artifact_headers_for_path("page.html").content_type, "application/octet-stream");
    assert!(artifact_headers_for_path("page.html").content_disposition.unwrap().starts_with("attachment"));
    assert_eq!(artifact_headers_for_path("logo.svg").content_type, "application/octet-stream");
}
```

Add an async symlink test:

```rust
#[tokio::test]
async fn symlink_component_under_output_root_is_forbidden() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("output");
    let screenshots = root.join("screenshots");
    tokio::fs::create_dir_all(&screenshots).await.unwrap();
    tokio::fs::write(screenshots.join("real.png"), b"png").await.unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(screenshots.join("real.png"), screenshots.join("alias.png")).unwrap();

    let err = validate_artifact_path_for_test(&root, "screenshots/alias.png")
        .await
        .expect_err("symlink should be rejected");
    assert_eq!(err.status(), StatusCode::FORBIDDEN);
}
```

If helper names differ during implementation, keep the assertions and expose test-only helpers under `#[cfg(test)]`.

- [x] **Step 2: Run the focused test and confirm failure**

Run:

```bash
cargo test artifacts_tests -- --nocapture
```

Expected: FAIL because backslash/colon/encoded traversal, active content attachment policy, and symlink-component rejection are not implemented yet.

- [x] **Step 3: Strengthen structural path validation**

Change `is_structurally_unsafe` in `src/web/server/handlers/artifacts.rs`:

```rust
pub(crate) fn is_structurally_unsafe(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\0') || path.contains('\\') {
        return true;
    }
    let decoded = percent_encoding::percent_decode_str(path)
        .decode_utf8_lossy();
    if decoded.contains(':') || decoded.contains('\\') {
        return true;
    }
    FsPath::new(decoded.as_ref()).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    })
}
```

If `percent-encoding` is not already a direct dependency, prefer the existing URL/percent-decoding utility in the repo. If no suitable utility exists, add the smallest direct dependency and keep it scoped.

- [x] **Step 4: Validate every path component before canonicalization**

Add a helper in `artifacts.rs`:

```rust
async fn reject_symlink_components(root: &Path, raw_path: &str) -> Result<(), HttpError> {
    let mut current = root.to_path_buf();
    let decoded = percent_encoding::percent_decode_str(raw_path).decode_utf8_lossy();
    for component in decoded.split('/') {
        if component.is_empty() {
            continue;
        }
        current.push(component);
        if let Ok(meta) = tokio::fs::symlink_metadata(&current).await
            && meta.file_type().is_symlink()
        {
            return Err(HttpError::new(
                StatusCode::FORBIDDEN,
                "symlink_not_allowed",
                "serving symlinked artifacts is not permitted",
            ));
        }
    }
    Ok(())
}
```

Call this before `tokio::fs::canonicalize(&candidate)`.

- [x] **Step 5: Stream artifact responses and add safe headers**

Replace whole-file reads with streaming:

```rust
let file = tokio::fs::File::open(&canonical_candidate).await.map_err(|e| {
    HttpError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "read_error",
        format!("failed to open artifact: {e}"),
    )
})?;
let stream = tokio_util::io::ReaderStream::new(file);
let body = Body::from_stream(stream);
let headers = artifact_headers_for_path(&raw_path);
Ok(headers.into_response(body))
```

Use a small local response builder if `headers.into_response(body)` is not the right shape. The response must include:

```rust
X-Content-Type-Options: nosniff
Content-Type: <safe content type>
Content-Disposition: attachment; filename="<leaf>" // for non-inline-safe types
```

Allowed inline image extensions: `png`, `jpg`, `jpeg`, `webp`, `gif`, `avif`. Serve `svg`, `html`, `htm`, and unknown extensions as opaque attachments. Serve `md`, `json`, `txt`, and `log` with accurate passive content types, still as attachments with `nosniff`, unless a later explicit safe text-preview feature is built.

- [x] **Step 6: Use the shared helper from panel artifact serving**

Make `serve_artifact_from_path` visible inside the server handlers module:

```rust
pub(crate) async fn serve_artifact_from_path(
    cfg: &crate::core::config::Config,
    raw_path: String,
) -> Result<Response, HttpError> {
    // shared validation, canonicalization, streaming response
}
```

In `src/web/server/handlers/config.rs`, replace the manual filesystem logic in `panel_artifact` with:

```rust
pub async fn panel_artifact(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
    Path(rel_path): Path<String>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }

    match super::artifacts::serve_artifact_from_path(&cfg, rel_path).await {
        Ok(response) => response,
        Err(err) => err.into_response(),
    }
}
```

Remove now-unused `Body` and `header` imports from `config.rs`.

- [x] **Step 7: Add panel route auth/content regression tests**

In the existing REST/panel auth test module, add a test equivalent to:

```rust
#[tokio::test]
async fn panel_artifact_requires_panel_token_and_serves_png() {
    let temp = tempfile::tempdir().unwrap();
    let screenshot_dir = temp.path().join("screenshots");
    std::fs::create_dir_all(&screenshot_dir).unwrap();
    std::fs::write(screenshot_dir.join("shot.png"), b"png-bytes").unwrap();

    let app = test_panel_router_with_output_dir(temp.path()).await;

    let unauthorized = app
        .clone()
        .oneshot(request("/api/panel/artifact/screenshots/shot.png"))
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let authorized = app
        .oneshot(
            request("/api/panel/artifact/screenshots/shot.png")
                .header("x-axon-panel-token", "test-panel-token"),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
    assert_eq!(authorized.headers().get(header::CONTENT_TYPE).unwrap(), "image/png");
    assert_eq!(authorized.headers().get("x-content-type-options").unwrap(), "nosniff");
}
```

Use the repo's existing router/test helper names rather than creating a second test harness if one already exists.

- [x] **Step 8: Verify backend artifact tests pass**

Run:

```bash
cargo test artifacts_tests panel_artifact -- --nocapture
```

Expected: PASS.

- [x] **Step 9: Commit**

```bash
git add src/web/server/handlers/artifacts.rs src/web/server/handlers/artifacts_tests.rs src/web/server/handlers/config.rs src/web/server/handlers/rest_auth_tests.rs Cargo.toml Cargo.lock
git commit -m "fix(web): stream safe artifact previews"
```

## Task 2: Fix Web Panel Artifact Preview URLs

**Files:**
- Modify: `apps/web/app/command-format.ts`
- Modify: `apps/web/app/panel-components.tsx`
- Test: create `apps/web/app/command-format.test.ts`

- [x] **Step 1: Write failing command-format tests**

Create `apps/web/app/command-format.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { formatCommandResponse, panelArtifactUrl } from './command-format';

describe('artifact preview URLs', () => {
  it('segment-encodes panel artifact paths', () => {
    expect(panelArtifactUrl('screenshots/foo #1.png')).toBe('/api/panel/artifact/screenshots/foo%20%231.png');
    expect(panelArtifactUrl('markdown/a%2Fb.md')).toBe('/api/panel/artifact/markdown/a%252Fb.md');
  });

  it('uses the panel artifact route for screenshot images', () => {
    const view = formatCommandResponse({
      command: 'screenshot https://example.com',
      action: { action: 'screenshot' },
      result: {
        url: 'https://example.com',
        path: '/home/axon/.axon/output/screenshots/example.png',
        size_bytes: 1024,
        artifact_handle: {
          relative_path: 'screenshots/example.png',
          display_path: 'screenshots/example.png',
          kind: 'screenshot',
          bytes: 1024
        }
      }
    });

    expect(view.imageUrl).toBe('/api/panel/artifact/screenshots/example.png');
    expect(view.raw).toBeUndefined();
  });

  it('does not create an image for non-image artifacts', () => {
    const view = formatCommandResponse({
      command: 'crawl https://example.com',
      action: { action: 'crawl' },
      result: {
        predicted_artifact_handles: [
          {
            relative_path: 'markdown/example.md',
            display_path: 'markdown/example.md',
            kind: 'markdown',
            bytes: 64
          }
        ]
      }
    });

    expect(view.imageUrl).toBeUndefined();
    expect(view.artifacts?.[0].relative_path).toBe('markdown/example.md');
  });
});
```

- [x] **Step 2: Run the web test and confirm failure**

Run:

```bash
cd apps/web
pnpm vitest run app/command-format.test.ts
```

Expected: FAIL because `imageUrl` currently uses `/v1/artifacts/<path>` and `ArtifactRow` builds raw URLs.

- [x] **Step 3: Add shared panel URL and image predicates**

In `apps/web/app/command-format.ts`, add exported helpers:

```ts
export function isPreviewableRasterArtifact(handle: ArtifactHandle): boolean {
  // Allow screenshot and raster image artifacts only; active image/* types such
  // as SVG are attachments, not inline previews.
  return handle.kind === 'screenshot' || isSafeRasterKindOrExtension(handle);
}

export function panelArtifactUrl(relativePath: string): string {
  return `/api/panel/artifact/${relativePath.split('/').map(encodeURIComponent).join('/')}`;
}
```

Then set the command result image URL with:

```ts
const handle = extractArtifactHandle(result);
const imageUrl = handle && isPreviewableRasterArtifact(handle) ? panelArtifactUrl(handle.relative_path) : undefined;
```

- [x] **Step 4: Use the shared helper in all panel artifact rows**

In `apps/web/app/panel-components.tsx`, import `panelArtifactUrl` and change:

```tsx
const src = `/api/panel/artifact/${artifact.relative_path}`;
```

to:

```tsx
const src = panelArtifactUrl(artifact.relative_path);
```

Use `src` for preview image source and artifact links. Do not build raw artifact URLs anywhere in this file.

- [x] **Step 5: Verify web tests**

Run:

```bash
cd apps/web
pnpm vitest run app/command-format.test.ts
pnpm tsc --noEmit
```

Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add apps/web/app/command-format.ts apps/web/app/command-format.test.ts apps/web/app/panel-components.tsx
git commit -m "fix(web): render artifact previews through panel auth"
```

## Task 3: Add A Dedicated Tauri Artifact Bridge And Palette Preview Loader

**Files:**
- Modify: `apps/palette-tauri/src-tauri/src/axon_bridge.rs`
- Test: `apps/palette-tauri/src-tauri/src/axon_bridge_tests.rs` or existing bridge test module
- Create: `apps/palette-tauri/src/lib/artifactPreview.ts`
- Modify: `apps/palette-tauri/src/components/palette/OperationResultView.tsx`
- Test: `apps/palette-tauri/src/components/palette/OperationResultView.test.tsx`

- [x] **Step 1: Write failing bridge tests**

Add Rust tests around the Tauri bridge:

```rust
#[test]
fn artifact_relative_path_validation_rejects_unsafe_values() {
    assert!(validate_artifact_relative_path("../secret").is_err());
    assert!(validate_artifact_relative_path(r"screenshots\\..\\secret").is_err());
    assert!(validate_artifact_relative_path("C:\\secret").is_err());
    assert!(validate_artifact_relative_path("screenshots/shot.png\0").is_err());
    assert!(validate_artifact_relative_path("screenshots/shot.png").is_ok());
}

#[test]
fn artifact_url_uses_query_encoding_without_accepting_raw_query_paths() {
    let url = artifact_url("https://axon.local", "screenshots/foo #1.png").unwrap();
    assert_eq!(url.as_str(), "https://axon.local/v1/artifacts?path=screenshots%2Ffoo+%231.png");
}
```

If the bridge tests use different helpers, expose these helpers under `#[cfg(test)]` or place tests beside the helper implementation.

- [x] **Step 2: Run bridge tests and confirm failure**

Run:

```bash
cd apps/palette-tauri/src-tauri
cargo test axon_bridge -- --nocapture
```

Expected: FAIL because no dedicated artifact bridge command/helper exists.

- [x] **Step 3: Add a narrow artifact bridge response type**

In `apps/palette-tauri/src-tauri/src/axon_bridge.rs`, add:

```rust
const MAX_ARTIFACT_PREVIEW_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, serde::Serialize)]
pub struct AxonArtifactResult {
    pub ok: bool,
    pub status: u16,
    pub content_type: String,
    pub body_base64: String,
}
```

Use a preview cap low enough to protect the renderer. If product needs larger downloads later, add a separate download/open flow.

- [x] **Step 4: Add strict relative-path validation and URL construction**

Add:

```rust
fn validate_artifact_relative_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path.contains('\\')
        || path.contains(':')
        || path.split('/').any(|part| part == ".." || part.is_empty())
    {
        return Err("artifact path must be a safe relative path".to_string());
    }
    Ok(())
}

fn artifact_url(base_url: &str, relative_path: &str) -> Result<url::Url, String> {
    validate_artifact_relative_path(relative_path)?;
    let mut url = url::Url::parse(base_url).map_err(|err| err.to_string())?;
    url.set_path("/v1/artifacts");
    url.query_pairs_mut().append_pair("path", relative_path);
    Ok(url)
}

fn is_allowed_artifact_content_type(value: &str) -> bool {
    matches!(
        value.split(';').next().unwrap_or("").trim(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif" | "image/avif"
    )
}
```

- [x] **Step 5: Add the Tauri command**

Add a command like:

```rust
#[tauri::command]
pub async fn axon_artifact_request(
    base_url: String,
    token: Option<String>,
    relative_path: String,
) -> Result<AxonArtifactResult, String> {
    let url = artifact_url(&base_url, &relative_path)?;
    let client = reqwest::Client::new();
    let mut request = client.get(url);
    if let Some(token) = token.filter(|value| !value.trim().is_empty()) {
        request = request
            .bearer_auth(token.trim())
            .header("x-api-key", token.trim());
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    if !status.is_success() {
        return Ok(AxonArtifactResult {
            ok: false,
            status: status.as_u16(),
            content_type,
            body_base64: String::new(),
        });
    }
    if !is_allowed_artifact_content_type(&content_type) {
        return Err("artifact content type is not previewable".to_string());
    }
    if response.content_length().unwrap_or(0) > MAX_ARTIFACT_PREVIEW_BYTES {
        return Err("artifact is too large to preview".to_string());
    }
    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
    if bytes.len() as u64 > MAX_ARTIFACT_PREVIEW_BYTES {
        return Err("artifact is too large to preview".to_string());
    }
    Ok(AxonArtifactResult {
        ok: true,
        status: status.as_u16(),
        content_type,
        body_base64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes),
    })
}
```

Add any required imports and register the command wherever `axon_http_request` is registered.

- [x] **Step 6: Create the TypeScript artifact preview loader**

Create `apps/palette-tauri/src/lib/artifactPreview.ts`:

```ts
import type { Client } from './axonClient';
import { invoke } from './invoke';

type ArtifactHttpResult = {
  ok: boolean;
  status: number;
  content_type: string;
  body_base64: string;
};

export async function loadArtifactObjectUrl(client: Client, relativePath: string): Promise<string> {
  const result = await invoke<ArtifactHttpResult>('axon_artifact_request', {
    baseUrl: client.baseUrl,
    token: client.headers.Authorization?.replace(/^Bearer\\s+/, '') ?? null,
    relativePath,
  });
  if (!result.ok) throw new Error(`artifact fetch failed with ${result.status}`);
  const binary = Uint8Array.from(atob(result.body_base64), (char) => char.charCodeAt(0));
  const blob = new Blob([binary], { type: result.content_type || 'application/octet-stream' });
  return URL.createObjectURL(blob);
}
```

- [x] **Step 7: Write failing palette render tests**

Add to `apps/palette-tauri/src/components/palette/OperationResultView.test.tsx`:

```tsx
it('renders screenshot artifact handles without relying on absolute server paths', async () => {
  render(
    <OperationResultView
      subcommand="screenshot"
      payload={{
        url: 'https://example.com',
        path: '/home/axon/.axon/output/screenshots/example.png',
        size_bytes: 1024,
        artifact_handle: {
          relative_path: 'screenshots/example.png',
          display_path: 'screenshots/example.png',
          kind: 'screenshot',
          bytes: 1024
        }
      }}
    />,
  );

  const img = await screen.findByRole('img', { name: /screenshot of https:\/\/example.com/i });
  expect(img).toHaveAttribute('src', expect.stringMatching(/^blob:|^data:image\//));
  expect(screen.queryByText('/home/axon/.axon/output/screenshots/example.png')).not.toBeInTheDocument();
});

it('shows a compact artifact preview failure state', async () => {
  mockLoadArtifactObjectUrl.mockRejectedValueOnce(new Error('artifact fetch failed with 401'));
  render(<OperationResultView subcommand="screenshot" payload={screenshotWithArtifactHandle()} />);
  expect(await screen.findByText(/preview unavailable/i)).toBeInTheDocument();
});
```

Mock `loadArtifactObjectUrl` to return `blob:test-shot` for the success test.

- [x] **Step 8: Render authenticated artifact images with race-safe cleanup**

In `OperationResultView.tsx`, add an `AuthenticatedArtifactImage` component. Use the existing client/config passed to the view or thread the existing `Client` down from the caller; do not invent a nonexistent `useAxonClientFromSettings()` hook.

The effect must:

- track a `cancelled` boolean;
- revoke a stale resolved URL immediately if the component unmounted;
- revoke the previous object URL before replacing it;
- render a compact “Preview unavailable” state on failure.

Use this component for screenshot payloads when `artifact_handle.relative_path` is present and no `preview_url`/`image_url`/`data_url` exists.

- [x] **Step 9: Keep absolute paths out of screenshot primary UI**

Change screenshot rendering and formatting so:

```tsx
<DetailLine label="Artifact" value={strField(artifact, "display_path") ?? relativePath ?? "-"} mono />
```

is shown instead of an absolute `Path` line. In `apps/palette-tauri/src/lib/format.ts`, remove the raw:

```ts
stringField(value, "path") ? `path: ${stringField(value, "path")}` : "",
```

from `screenshotReport`. Keep absolute `path` only in raw JSON/debug surfaces.

- [x] **Step 10: Verify palette bridge and UI tests**

Run:

```bash
cd apps/palette-tauri/src-tauri
cargo test axon_bridge -- --nocapture
cd ..
pnpm vitest run src/components/palette/OperationResultView.test.tsx src/lib/format.test.ts
pnpm tsc --noEmit
```

Expected: PASS.

- [x] **Step 11: Commit**

```bash
git add apps/palette-tauri/src-tauri/src/axon_bridge.rs apps/palette-tauri/src-tauri/src/axon_bridge_tests.rs apps/palette-tauri/src/lib/artifactPreview.ts apps/palette-tauri/src/components/palette/OperationResultView.tsx apps/palette-tauri/src/components/palette/OperationResultView.test.tsx apps/palette-tauri/src/lib/format.ts apps/palette-tauri/src/lib/format.test.ts
git commit -m "fix(palette): preview artifacts through a capped bridge"
```

## Task 4: Document And Verify The Artifact Result Contract

**Files:**
- Modify: `docs/reference/job-lifecycle.md`
- Modify: `docs/reference/http-api.md`
- Test: existing Rust and app test suites touched above

- [x] **Step 1: Document the artifact app UX contract**

Add to `docs/reference/job-lifecycle.md`:

```markdown
## App Artifact UX Contract

Automation-facing REST job submission routes keep returning `202 AcceptedJob`.
This artifact UX contract does not change job submission semantics.

Artifact handles are the app contract. Absolute `path` fields are server-host
debug metadata. Web panel previews fetch `/api/panel/artifact/{relative_path}`
with panel auth and render object URLs. REST clients use
`GET /v1/artifacts?path=...` with normal `axon:read` auth. Tauri palette previews
fetch raster image bytes through the capped artifact bridge command and render
object URLs.

Only raster image artifacts are previewed inline. Active or ambiguous artifact
types such as HTML and SVG are served as opaque attachments with `nosniff`.
JSON, markdown, text, and logs keep accurate passive content types but are still
served as attachments.
```

- [x] **Step 2: Document artifact route usage in HTTP API reference**

In `docs/reference/http-api.md`, add:

```markdown
Artifact download:

- `GET /v1/artifacts?path=<relative_path>` serves files under `output_dir` and
  requires read auth.
- Clients must pass the `relative_path` from an `ArtifactHandle`; absolute server
  paths are not accepted.
- Browser apps must fetch authenticated bytes and render object URLs; image tags
  must not point directly at authenticated artifact routes.
- Only raster image artifacts are inline preview content. HTML, SVG, unknown
  types, JSON, markdown, text, and logs are served as attachments with `nosniff`.
```

- [x] **Step 3: Run backend verification**

Run:

```bash
cargo test artifacts_tests panel_artifact screenshot -- --nocapture
cargo fmt --all -- --check
```

Expected: PASS.

- [x] **Step 4: Run app verification**

Run:

```bash
cd apps/web
npm test
pnpm tsc --noEmit
cd ../palette-tauri/src-tauri
cargo test axon_bridge -- --nocapture
cd ..
pnpm vitest run src/components/palette/OperationResultView.test.tsx src/lib/format.test.ts
pnpm tsc --noEmit
```

Expected: PASS.

- [x] **Step 5: Run final broad checks if time permits**

Run:

```bash
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Expected: PASS. If the known cargo wrapper drift reproduces an `E0514` mixed-rustc failure, prove it with a fresh target dir and explicit wrapper override before treating it as a code failure.

- [x] **Step 6: Commit**

```bash
git add docs/reference/job-lifecycle.md docs/reference/http-api.md
git commit -m "docs(app): document artifact result UX contract"
```

## Deferred Follow-Up

Deferred bead: `axon_rust-gnb6.5` for generalized user-facing app job polling.

- Scope: poll app-triggered crawl/embed/extract/ingest jobs to terminal state.
- Requirements: one foreground poller per active run, abortable cleanup, backoff, max duration, visible failure state, terminal `result_json` and artifact rendering, tests for stale/unmount/server-down behavior.
- Non-goal for this plan: changing `/v1/crawl`, `/v1/embed`, `/v1/extract`, or `/v1/ingest` `202 AcceptedJob` semantics.

Additional post-review follow-up candidates:

- Add command-level HTTP mock tests for `axon_artifact_request` auth header forwarding, non-2xx response shape, rejected response content types, and oversized `Content-Length`.
- Add direct route-level tests for `serve_artifact_from_path` streaming headers and legacy path/query route parity. The dangerous resolver/header boundaries are covered in this plan; these route tests are lower-risk belt-and-suspenders coverage.

## Self-Review

Spec coverage:

- Screenshot artifacts render as images: Tasks 2 and 3.
- Artifact handles are canonical: Tasks 1, 2, 3, and 4.
- Async REST endpoints remain unchanged: all tasks explicitly avoid changing job submission semantics.
- Web panel and palette avoid raw absolute paths as the primary contract: Tasks 2 and 3.
- Security review findings are covered: active content attachment policy, `nosniff`, stronger path validation, symlink-component checks, and dedicated Tauri artifact bridge.
- Performance review findings are covered: streaming artifact responses, preview byte cap, object URL cleanup, and deferral of broad polling.
- Final PR-review findings are covered: web Vitest runs in CI, web component tests cover authenticated fetch/error behavior, preview artifacts remain downloadable when preview fails, and web/Tauri preview failures preserve specific error messages.

Placeholder scan:

- No task uses TBD, placeholder instructions, or test steps without concrete assertions.

Type consistency:

- `ArtifactHandle.relative_path`, `display_path`, `kind`, and `bytes` match `apps/web/app/panel-types.ts` and `src/services/types/client_server.rs`.
- Tauri artifact bridge response is explicitly defined as `{ ok, status, content_type, body_base64 }`.
