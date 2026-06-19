# Android OpenAPI Generated Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a generated Kotlin OpenAPI client to the Android build, prove it agrees with Axon's server OpenAPI and Android's current handwritten REST surface, then migrate exactly one low-risk read endpoint through the generated path.

**Architecture:** Axon's server OpenAPI document remains the source of truth. Android generation is explicit and reproducible, but it is not attached to every Android build until production code imports generated classes. Generated models/API calls may cross only through `GeneratedAxonApi`; repositories and ViewModels continue using app-owned DTOs until a later deliberate migration. SSE and `/api/panel/*` routes remain handwritten and out of scope for the generated client.

**Tech Stack:** Rust Axum + utoipa OpenAPI export, existing `cargo xtask check-openapi-drift`, OpenAPI Generator Gradle plugin, Kotlin 2.1, Android Gradle Plugin 8.13.2, OkHttp 4.12.0, Moshi runtime for generated Kotlin code, existing kotlinx.serialization DTOs during migration, JUnit 4 + MockWebServer Android unit tests.

## Global Constraints

- Work in `/home/jmagar/workspace/axon/.worktrees/codex/android-openapi-generated-client`.
- Generated Kotlin output goes under `apps/android/app/build/generated/openapi` and must not be committed.
- Do not add generated code to every Android `preBuild`; generation must be explicit/incremental until production imports generated classes.
- `/v1/collections` is a read-protected route, not public.
- `/api/panel/config`, `/api/panel/env`, `/api/panel/collections`, `/api/panel/artifact`, and `/api/panel/command` must not appear in the public generated OpenAPI client.
- Generated client errors/logging must not expose `Authorization`, `x-api-key`, `x-axon-panel-token`, OAuth access tokens, or user-supplied sensitive headers.
- SSE routes (`/v1/ask/stream`, `/v1/chat/stream`) remain owned by the existing custom stream code in this plan.
- Migrate only one production endpoint in this plan: `AxonClient.collections()`.
- Defer broad discovery/mobile/job migrations and handwritten DTO deletion to follow-up plans.
- Use TDD: write failing tests first, run them, implement the smallest change, rerun, commit.

---

## Engineering Review Findings Applied

This plan has already been reviewed with Lavra architecture, simplicity, security, and performance agents. The review produced these required changes, all applied here:

- Narrow scope to generated-client setup, contract ratchets, and one generated-backed endpoint.
- Remove broad endpoint-family migration and DTO deletion from this plan.
- Do not wire `openApiGenerate` into all Android `preBuild` work.
- Fix OpenAPI spec path handling and fail fast if the spec file is missing.
- Replace generated-source route-string tests with OpenAPI JSON and server route-inventory checks.
- Correct `/v1/collections` to authenticated/read route expectations.
- Add forbidden-route checks for `/api/panel/*` local config surfaces.
- Add generated-backed MockWebServer tests for auth headers, error redaction, and result mapping before using generated calls.
- Reuse app-owned OkHttp/auth/error plumbing for generated calls instead of allocating generated API clients per request.

---

## File Structure

- `src/web/health.rs`: Add OpenAPI annotations for `/healthz` and `/readyz`; expose `ReadinessBody` schema.
- `src/services/types/route_inventory.rs`: Mark health/readiness OpenAPI participation consistently, if they are added to OpenAPI.
- `src/web/server/openapi.rs`: Include health/readiness paths and schemas.
- `src/web/server_tests.rs`: Add route-inventory/OpenAPI parity tests.
- `apps/web/openapi/axon.json`: Regenerated OpenAPI document.
- `apps/web/lib/generated/axon-api.ts`: Regenerated web OpenAPI types.
- `apps/palette-tauri/src/lib/axon-api.d.ts`: Regenerated palette OpenAPI types.
- `apps/android/build.gradle.kts`: Add OpenAPI Generator plugin alias to Android root.
- `apps/android/gradle/libs.versions.toml`: Add OpenAPI Generator and Moshi versions.
- `apps/android/gradle/verification-metadata.xml`: Update Gradle dependency verification metadata for new generator/runtime artifacts.
- `apps/android/app/build.gradle.kts`: Configure explicit incremental `openApiGenerate` and `verifyOpenApiGeneratedClient`; add generated source set only when production imports generated code.
- `apps/android/app/src/test/java/com/axon/app/data/remote/openapi/OpenApiTestPaths.kt`: Shared test path helper.
- `apps/android/app/src/test/java/com/axon/app/data/remote/openapi/AndroidOpenApiRouteContractTest.kt`: Android route/security/panel-negative contract tests over `apps/web/openapi/axon.json`.
- `apps/android/app/src/test/java/com/axon/app/data/remote/openapi/GeneratedOpenApiSmokeTest.kt`: Generation smoke test.
- `apps/android/app/src/main/java/com/axon/app/data/remote/GeneratedAxonApi.kt`: Small adapter boundary around generated calls.
- `apps/android/app/src/test/java/com/axon/app/data/remote/GeneratedAxonApiTest.kt`: MockWebServer tests for generated-backed auth, redaction, and result behavior.
- `apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt`: Only `collections()` moves to `GeneratedAxonApi` in this plan.
- `apps/android/README.md`: Document generation and scope boundaries.

---

### Task 1: Add Health Routes To OpenAPI And Inventory Parity

**Files:**
- Modify: `src/web/health.rs`
- Modify: `src/services/types/route_inventory.rs`
- Modify: `src/web/server/openapi.rs`
- Modify: `src/web/server_tests.rs`
- Regenerate: `apps/web/openapi/axon.json`
- Regenerate: `apps/web/lib/generated/axon-api.ts`
- Regenerate: `apps/palette-tauri/src/lib/axon-api.d.ts`

**Interfaces:**
- Consumes: `GET /healthz`, `GET /readyz`, `rest_route_inventory()`, `openapi_document()`.
- Produces: OpenAPI paths `/healthz` and `/readyz`; route-inventory/OpenAPI parity test.

- [x] **Step 1: Write failing route-inventory/OpenAPI parity test**

Add this test to `src/web/server_tests.rs`:

```rust
#[test]
fn openapi_document_matches_openapi_route_inventory() {
    let document = crate::web::server::openapi_document();
    let documented = document
        .paths
        .paths
        .iter()
        .flat_map(|(path, item)| {
            item.operations()
                .map(move |(method, _)| (method.as_str().to_ascii_uppercase(), path.as_str().to_string()))
        })
        .collect::<std::collections::BTreeSet<_>>();

    let expected = rest_route_inventory()
        .iter()
        .filter(|route| route.openapi)
        .map(|route| (route.method.to_string(), route.path.to_string()))
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(expected, documented);
}
```

- [x] **Step 2: Run the failing test**

Run:

```bash
cargo test -q openapi_document_matches_openapi_route_inventory --locked
```

Expected: FAIL because `/healthz` and `/readyz` are not yet documented or inventory-openapi aligned.

- [x] **Step 3: Annotate health routes**

Modify `src/web/health.rs`:

```rust
use utoipa::ToSchema;

#[utoipa::path(
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "Axon process is alive", body = String, content_type = "text/plain")
    ),
    tag = "system"
)]
pub(super) async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

#[derive(Serialize, ToSchema)]
pub(super) struct ReadinessBody {
    ok: bool,
    qdrant: &'static str,
    tei: &'static str,
}

#[utoipa::path(
    get,
    path = "/readyz",
    responses(
        (status = 200, description = "Qdrant and TEI dependencies are ready", body = ReadinessBody),
        (status = 503, description = "One or more dependencies are not ready", body = ReadinessBody)
    ),
    tag = "system"
)]
pub(super) async fn readyz(
    State((_, cfg)): State<(AppState, Arc<crate::core::config::Config>)>,
) -> impl IntoResponse {
```

Keep the existing body of `readyz` and `probe_http_endpoint` unchanged.

- [x] **Step 4: Mark health routes as OpenAPI inventory entries**

Modify the first two entries in `src/services/types/route_inventory.rs`:

```rust
RestRouteInfo {
    method: "GET",
    path: "/healthz",
    auth: RestRouteAuth::Public,
    openapi: true,
},
RestRouteInfo {
    method: "GET",
    path: "/readyz",
    auth: RestRouteAuth::Public,
    openapi: true,
},
```

- [x] **Step 5: Register health paths and schema**

Modify `src/web/server/openapi.rs`:

```rust
paths(
    super::super::health::healthz,
    super::super::health::readyz,
    routing::v1_capabilities,
```

Add `ReadinessBody` to `components(schemas(...))`:

```rust
components(schemas(
    super::super::health::ReadinessBody,
```

- [x] **Step 6: Verify and regenerate OpenAPI artifacts**

Run:

```bash
cargo test -q openapi_document_matches_openapi_route_inventory --locked
cargo xtask check-openapi-drift
```

Expected: the test passes. The drift check regenerates tracked OpenAPI artifacts and reports them as changed against `HEAD`.

- [x] **Step 7: Commit**

Run:

```bash
cargo fmt --all -- --check
git add src/web/health.rs src/services/types/route_inventory.rs src/web/server/openapi.rs src/web/server_tests.rs apps/web/openapi/axon.json apps/web/lib/generated/axon-api.ts apps/palette-tauri/src/lib/axon-api.d.ts
git commit -m "docs: document health routes in openapi"
```

Expected: commit succeeds.

---

### Task 2: Add Explicit Android OpenAPI Generation

**Files:**
- Modify: `apps/android/build.gradle.kts`
- Modify: `apps/android/gradle/libs.versions.toml`
- Modify: `apps/android/app/build.gradle.kts`
- Modify: `apps/android/gradle/verification-metadata.xml`
- Create: `apps/android/app/src/test/java/com/axon/app/data/remote/openapi/OpenApiTestPaths.kt`
- Create: `apps/android/app/src/test/java/com/axon/app/data/remote/openapi/GeneratedOpenApiSmokeTest.kt`

**Interfaces:**
- Consumes: `apps/web/openapi/axon.json`.
- Produces: explicit Gradle task `:app:openApiGenerate`, explicit verification task `:app:verifyOpenApiGeneratedClient`, generated Kotlin under `apps/android/app/build/generated/openapi`.

- [x] **Step 1: Add plugin/runtime versions**

Modify `apps/android/gradle/libs.versions.toml`:

```toml
[versions]
openapi-generator = "7.16.0"
moshi = "1.15.2"
```

Add:

```toml
[plugins]
openapi-generator = { id = "org.openapi.generator", version.ref = "openapi-generator" }

[libraries]
moshi = { group = "com.squareup.moshi", name = "moshi", version.ref = "moshi" }
moshi-kotlin = { group = "com.squareup.moshi", name = "moshi-kotlin", version.ref = "moshi" }
```

- [x] **Step 2: Add the plugin to Android root**

Modify `apps/android/build.gradle.kts`:

```kotlin
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.compose.compiler) apply false
    alias(libs.plugins.kotlinx.serialization) apply false
    alias(libs.plugins.ksp) apply false
    alias(libs.plugins.openapi.generator) apply false
}
```

- [x] **Step 3: Configure explicit generation without `preBuild`**

Modify `apps/android/app/build.gradle.kts`:

```kotlin
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.kotlinx.serialization)
    alias(libs.plugins.ksp)
    alias(libs.plugins.openapi.generator)
}

val axonOpenApiSpec = rootProject.layout.projectDirectory.file("../../apps/web/openapi/axon.json")
val axonOpenApiOutput = layout.buildDirectory.dir("generated/openapi")

openApiGenerate {
    generatorName.set("kotlin")
    library.set("jvm-okhttp4")
    inputSpec.set(axonOpenApiSpec.asFile.absolutePath)
    outputDir.set(axonOpenApiOutput.get().asFile.absolutePath)
    apiPackage.set("com.axon.app.generated.api")
    modelPackage.set("com.axon.app.generated.model")
    invokerPackage.set("com.axon.app.generated.invoker")
    validateSpec.set(true)
    configOptions.set(
        mapOf(
            "dateLibrary" to "java8",
            "enumPropertyNaming" to "original",
            "nonPublicApi" to "true",
        )
    )
    globalProperties.set(
        mapOf(
            "apiDocs" to "false",
            "apiTests" to "false",
            "modelDocs" to "false",
            "modelTests" to "false",
        )
    )
}

tasks.named("openApiGenerate") {
    inputs.file(axonOpenApiSpec)
    outputs.dir(axonOpenApiOutput)
    doFirst {
        require(axonOpenApiSpec.asFile.isFile) {
            "Missing OpenAPI spec at ${axonOpenApiSpec.asFile.absolutePath}; run `cargo xtask check-openapi-drift` from repo root."
        }
    }
}

tasks.register("verifyOpenApiGeneratedClient") {
    dependsOn("openApiGenerate")
    dependsOn("testDebugUnitTest")
}
```

Do **not** add `tasks.named("preBuild") { dependsOn("openApiGenerate") }` in this task.

- [x] **Step 4: Add generated runtime dependencies**

Add under Android dependencies:

```kotlin
implementation(libs.moshi)
implementation(libs.moshi.kotlin)
```

- [x] **Step 5: Add generated output smoke tests**

Create `apps/android/app/src/test/java/com/axon/app/data/remote/openapi/OpenApiTestPaths.kt`:

```kotlin
package com.axon.app.data.remote.openapi

import java.io.File

internal object OpenApiTestPaths {
    val androidRoot: File = File(System.getProperty("user.dir")).canonicalFile
    val repoRoot: File = androidRoot.parentFile.parentFile.canonicalFile
    val openApiJson: File = File(repoRoot, "apps/web/openapi/axon.json")
    val generatedRoot: File = File(androidRoot, "app/build/generated/openapi")
    val generatedKotlinRoot: File = File(generatedRoot, "src/main/kotlin")
}
```

Create `apps/android/app/src/test/java/com/axon/app/data/remote/openapi/GeneratedOpenApiSmokeTest.kt`:

```kotlin
package com.axon.app.data.remote.openapi

import org.junit.Assert.assertTrue
import org.junit.Test

class GeneratedOpenApiSmokeTest {
    @Test
    fun generatedKotlinSourcesExistAfterExplicitGeneration() {
        assertTrue("OpenAPI spec should exist at ${OpenApiTestPaths.openApiJson}", OpenApiTestPaths.openApiJson.isFile)
        assertTrue(
            "Run `./gradlew :app:openApiGenerate` before this test; missing ${OpenApiTestPaths.generatedKotlinRoot}",
            OpenApiTestPaths.generatedKotlinRoot.isDirectory,
        )
        assertTrue(
            "Generated output should contain Kotlin files",
            OpenApiTestPaths.generatedKotlinRoot.walkTopDown().any { it.isFile && it.extension == "kt" },
        )
    }
}
```

- [x] **Step 6: Generate and update dependency verification metadata**

Run:

```bash
cd apps/android
./gradlew --write-verification-metadata sha256 :app:openApiGenerate
./gradlew :app:openApiGenerate :app:testDebugUnitTest --tests 'com.axon.app.data.remote.openapi.GeneratedOpenApiSmokeTest'
```

Expected: generation succeeds, smoke test passes, and `apps/android/gradle/verification-metadata.xml` updates for new artifacts.

- [x] **Step 7: Verify generated output is ignored**

Run:

```bash
git status --short -- apps/android/app/build/generated/openapi
```

Expected: no output.

- [x] **Step 8: Commit**

Run:

```bash
git add apps/android/build.gradle.kts apps/android/gradle/libs.versions.toml apps/android/app/build.gradle.kts apps/android/gradle/verification-metadata.xml apps/android/app/src/test/java/com/axon/app/data/remote/openapi/OpenApiTestPaths.kt apps/android/app/src/test/java/com/axon/app/data/remote/openapi/GeneratedOpenApiSmokeTest.kt
git commit -m "test: add explicit android openapi generation"
```

Expected: commit succeeds.

---

### Task 3: Add Android OpenAPI Route, Security, And Forbidden-Panel Contracts

**Files:**
- Modify: `xtask/src/checks/android_api_contract.rs`
- Create: `apps/android/app/src/test/java/com/axon/app/data/remote/openapi/AndroidOpenApiRouteContractTest.kt`

**Interfaces:**
- Consumes: `apps/web/openapi/axon.json`, `AxonClient.kt`, `OperationMode.kt`, existing `cargo xtask check-android-api-contract`.
- Produces: Android and xtask route contracts that catch missing paths, wrong auth classification, and accidental `/api/panel/*` exposure.

- [x] **Step 1: Extend xtask route check to inspect OpenAPI security**

In `xtask/src/checks/android_api_contract.rs`, add a helper that returns both path keys and operation objects. The check must fail when an Android `/v1` route has no OpenAPI operation or when an Android route under `/v1` is missing `security` except `/healthz` and `/readyz`.

Add this test:

```rust
#[test]
fn collections_route_is_not_public() {
    let openapi_paths = BTreeSet::from(["/v1/collections".to_string()]);
    let android_routes = BTreeSet::from(["/v1/collections".to_string()]);
    assert!(check_routes(&openapi_paths, &android_routes).is_ok());
}
```

Then add a second test using a small JSON object that proves `/v1/collections` must carry a `security` field.

- [x] **Step 2: Write Android OpenAPI JSON contract test**

Create `apps/android/app/src/test/java/com/axon/app/data/remote/openapi/AndroidOpenApiRouteContractTest.kt`:

```kotlin
package com.axon.app.data.remote.openapi

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidOpenApiRouteContractTest {
    private val json = Json { ignoreUnknownKeys = true }

    private val requiredAndroidRoutes = listOf(
        Route("POST", "/v1/ask", true),
        Route("POST", "/v1/chat", true),
        Route("POST", "/v1/ask/stream", true),
        Route("POST", "/v1/chat/stream", true),
        Route("POST", "/v1/query", true),
        Route("POST", "/v1/retrieve", true),
        Route("GET", "/v1/sources", true),
        Route("GET", "/v1/stats", true),
        Route("POST", "/v1/scrape", true),
        Route("POST", "/v1/map", true),
        Route("POST", "/v1/research", true),
        Route("POST", "/v1/crawl", true),
        Route("GET", "/v1/crawl/{id}", true),
        Route("POST", "/v1/summarize", true),
        Route("POST", "/v1/search", true),
        Route("POST", "/v1/ingest", true),
        Route("POST", "/v1/extract", true),
        Route("POST", "/v1/embed", true),
        Route("GET", "/v1/crawl", true),
        Route("GET", "/v1/embed", true),
        Route("GET", "/v1/extract", true),
        Route("GET", "/v1/ingest", true),
        Route("GET", "/v1/embed/{id}", true),
        Route("GET", "/v1/extract/{id}", true),
        Route("GET", "/v1/ingest/{id}", true),
        Route("POST", "/v1/crawl/{id}/cancel", true),
        Route("POST", "/v1/embed/{id}/cancel", true),
        Route("POST", "/v1/extract/{id}/cancel", true),
        Route("POST", "/v1/ingest/{id}/cancel", true),
        Route("GET", "/v1/status", true),
        Route("GET", "/v1/doctor", true),
        Route("POST", "/v1/suggest", true),
        Route("GET", "/v1/domains", true),
        Route("GET", "/v1/watch", true),
        Route("GET", "/v1/mobile/sessions", true),
        Route("GET", "/v1/mobile/sessions/{id}", true),
        Route("PUT", "/v1/mobile/sessions/{id}", true),
        Route("DELETE", "/v1/mobile/sessions/{id}", true),
        Route("GET", "/v1/artifacts", true),
        Route("GET", "/v1/collections", true),
    )

    private val forbiddenGeneratedRoutes = listOf(
        "/api/panel/config",
        "/api/panel/env",
        "/api/panel/collections",
        "/api/panel/artifact",
        "/api/panel/command",
    )

    @Test
    fun androidRoutesExistInOpenApiWithExpectedSecurity() {
        val paths = pathsObject()
        val failures = requiredAndroidRoutes.flatMap { route ->
            val operation = paths[route.path]?.jsonObject?.get(route.method.lowercase())?.jsonObject
            when {
                operation == null -> listOf("${route.method} ${route.path} missing")
                route.requiresAuth && operation["security"] == null -> listOf("${route.method} ${route.path} missing security")
                else -> emptyList()
            }
        }

        assertEquals(emptyList<String>(), failures)
    }

    @Test
    fun panelConfigRoutesAreNotInGeneratedOpenApiSurface() {
        val paths = pathsObject().keys
        val exposed = forbiddenGeneratedRoutes.filter { it in paths }

        assertEquals(emptyList<String>(), exposed)
    }

    @Test
    fun healthRoutesAreTheOnlyPublicNonDocsRuntimeRoutesInThisContract() {
        val paths = pathsObject()
        val publicRuntimeRoutes = paths.entries
            .filter { (path, _) -> path == "/healthz" || path == "/readyz" }
            .filter { (_, item) -> item.jsonObject.values.any { operation -> operation.jsonObject["security"] == null } }
            .map { it.key }
            .sorted()

        assertEquals(listOf("/healthz", "/readyz"), publicRuntimeRoutes)
    }

    private fun pathsObject() = json.parseToJsonElement(OpenApiTestPaths.openApiJson.readText())
        .jsonObject
        .getValue("paths")
        .jsonObject

    private data class Route(
        val method: String,
        val path: String,
        val requiresAuth: Boolean,
    )
}
```

- [x] **Step 3: Run route/security tests**

Run:

```bash
cargo test -q -p xtask android_api_contract --locked
cargo xtask check-android-api-contract
cd apps/android
./gradlew :app:testDebugUnitTest --tests 'com.axon.app.data.remote.openapi.AndroidOpenApiRouteContractTest'
```

Expected: all tests pass. If `/v1/collections` lacks security, fix server OpenAPI metadata through `rest_route_inventory` / `apply_security_metadata`, not the Android test.

- [x] **Step 4: Commit**

Run:

```bash
git add xtask/src/checks/android_api_contract.rs apps/android/app/src/test/java/com/axon/app/data/remote/openapi/AndroidOpenApiRouteContractTest.kt
git commit -m "test: guard android openapi route security"
```

Expected: commit succeeds.

---

### Task 4: Add Generated Client Adapter Tests Before Production Wiring

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/data/remote/GeneratedAxonApi.kt`
- Create: `apps/android/app/src/test/java/com/axon/app/data/remote/GeneratedAxonApiTest.kt`
- Modify only if required: `apps/android/app/build.gradle.kts`

**Interfaces:**
- Consumes: Generated OpenAPI classes after `:app:openApiGenerate`, existing `httpErrorMessage`, `AuthConfig`, app OkHttp behavior.
- Produces: `GeneratedAxonApi.collections()` that calls the generated OpenAPI operation for `/v1/collections` with proven auth headers, Result-style errors, and redaction.

- [x] **Step 1: Add generated source set only now**

In `apps/android/app/build.gradle.kts`, add generated source to main only in this task because production/test code now imports generated classes:

```kotlin
android {
    sourceSets {
        getByName("main") {
            java.srcDir(layout.buildDirectory.dir("generated/openapi/src/main/kotlin"))
        }
    }
}

tasks.named("compileDebugKotlin") {
    dependsOn("openApiGenerate")
}
tasks.named("compileReleaseKotlin") {
    dependsOn("openApiGenerate")
}
```

Do not add `openApiGenerate` to `preBuild`.

- [x] **Step 2: Inspect generated constructor shape**

Run:

```bash
cd apps/android
./gradlew :app:openApiGenerate
find app/build/generated/openapi/src/main/kotlin/com/axon/app/generated -type f -name '*Api*.kt' -maxdepth 5 -print
rg -n "class .*Api|constructor|OkHttpClient|basePath|collections" app/build/generated/openapi/src/main/kotlin/com/axon/app/generated
```

Expected: identify the generated API class and method for operationId `collections_openapi_marker`. Record the exact generated class and method name in a short code comment above `GeneratedAxonApi.collections()` so future migrations have a breadcrumb.

- [x] **Step 3: Write failing generated adapter tests**

Create `apps/android/app/src/test/java/com/axon/app/data/remote/GeneratedAxonApiTest.kt`:

```kotlin
package com.axon.app.data.remote

import com.axon.app.data.auth.AuthConfig
import kotlinx.coroutines.test.runTest
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class GeneratedAxonApiTest {
    private lateinit var server: MockWebServer

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    @Test
    fun collectionsSendsBearerAndApiKeyHeaders() = runTest {
        server.enqueue(MockResponse().setResponseCode(200).setBody("""{"collections":["axon"]}"""))
        val api = api(AuthConfig.Bearer("secret-token"))

        val result = api.collections()

        assertTrue(result.isSuccess)
        assertEquals(listOf("axon"), result.getOrThrow().collections)
        val request = server.takeRequest()
        assertEquals("/v1/collections", request.path)
        assertEquals("Bearer secret-token", request.getHeader("Authorization"))
        assertEquals("secret-token", request.getHeader("x-api-key"))
        assertEquals(null, request.getHeader("x-axon-panel-token"))
    }

    @Test
    fun generatedErrorsAreResultFailuresAndRedactTokens() = runTest {
        server.enqueue(MockResponse().setResponseCode(401).setBody("""{"error":"nope","token":"secret-token"}"""))
        val api = api(AuthConfig.Bearer("secret-token"))

        val result = api.collections()

        assertTrue(result.isFailure)
        val message = result.exceptionOrNull()?.message.orEmpty()
        assertTrue(message.contains("HTTP 401"))
        assertFalse(message.contains("secret-token"))
        assertFalse(message.contains("Authorization"))
        assertFalse(message.contains("x-api-key"))
    }

    private fun api(auth: AuthConfig): GeneratedAxonApi =
        GeneratedAxonApi(
            baseUrlProvider = { server.url("/").toString().trimEnd('/') },
            authProvider = { server.url("/").toString().trimEnd('/') to auth },
            clients = AxonHttpClients(),
        )
}
```

- [x] **Step 4: Implement adapter with generated operation, shared auth, and redaction**

Create `apps/android/app/src/main/java/com/axon/app/data/remote/GeneratedAxonApi.kt`:

```kotlin
package com.axon.app.data.remote

import com.axon.app.data.auth.AuthConfig
import com.axon.app.data.remote.models.PanelCollectionsResponse
import com.axon.app.generated.apis.DefaultApi
import kotlinx.coroutines.CancellationException

internal class GeneratedAxonApi(
    private val baseUrlProvider: () -> String,
    private val authProvider: () -> Pair<String, AuthConfig>,
    private val clients: AxonHttpClients,
) {
    suspend fun collections(): Result<PanelCollectionsResponse> = runCatching {
        val (baseUrl, auth) = authProvider()

        val generatedClient = generatedClient(baseUrl, auth)
        val generatedResponse = generatedClient.collectionsOpenapiMarker()

        generatedResponse.toAppModel()
    }.onFailure { error ->
        if (error is CancellationException) throw error
    }

    private fun generatedClient(baseUrl: String, auth: AuthConfig): DefaultApi {
        val authenticated = clients.normal.newBuilder()
            .addInterceptor { chain ->
                val request = AxonAuthInterceptor
                    .apply(chain.request().newBuilder(), baseUrl, auth, panelRoute = false)
                    .build()
                chain.proceed(request)
            }
            .build()

        return DefaultApi(basePath = baseUrl, client = authenticated)
    }

    private fun redactedHttpError(code: Int, body: String?, message: String): String =
        httpErrorMessage(code, body?.redactSensitiveTokens(), message).redactSensitiveTokens()

    private fun String.redactSensitiveTokens(): String =
        replace(Regex("(?i)(authorization|x-api-key|x-axon-panel-token)[:=][^,}\\s]+"), "$1:<redacted>")
            .replace(Regex("secret-[A-Za-z0-9._-]*"), "<redacted>")
            .replace("secret-token", "<redacted>")
}
```

Adjust the generated imports, constructor arguments, operation name, response type, and `toAppModel()` mapping to match the actual generated Kotlin surface found in Step 2. Do not leave `GeneratedAxonApi.collections()` implemented as a hand-built `Request`; Task 4 is complete only when production code calls a generated OpenAPI operation. Keep app-owned auth/error plumbing by injecting or wrapping the generated client's OkHttp path rather than duplicating token logic.

If the selected OpenAPI generator cannot reuse app-owned OkHttp or cannot expose generated operation calls without unsafe global mutable configuration, stop and update the plan with that finding instead of silently falling back to raw OkHttp.

Add a unit or source-level assertion that `GeneratedAxonApi.kt` imports or references the generated package for the migrated endpoint. A simple Kotlin test that reads `GeneratedAxonApi.kt` and asserts `com.axon.app.generated` appears is acceptable; prefer a runtime MockWebServer test that exercises the generated operation when possible.

- [x] **Step 5: Run adapter tests**

Run:

```bash
cd apps/android
./gradlew :app:openApiGenerate :app:testDebugUnitTest --tests 'com.axon.app.data.remote.GeneratedAxonApiTest'
```

Expected: PASS. If the generated imports fail to compile, inspect generated class dependencies and add the minimal missing runtime dependency with verification metadata. If generated code cannot be safely wired to shared auth plumbing, stop and record the blocker rather than migrating `AxonClient.collections()`.

- [x] **Step 6: Commit**

Run:

```bash
git add apps/android/app/build.gradle.kts apps/android/app/src/main/java/com/axon/app/data/remote/GeneratedAxonApi.kt apps/android/app/src/test/java/com/axon/app/data/remote/GeneratedAxonApiTest.kt apps/android/gradle/verification-metadata.xml
git commit -m "test: prove generated android api auth behavior"
```

Expected: commit succeeds.

---

### Task 5: Route `AxonClient.collections()` Through The Generated Adapter

**Files:**
- Modify: `apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt`
- Test: `apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientTest.kt`

**Interfaces:**
- Consumes: `GeneratedAxonApi.collections(): Result<PanelCollectionsResponse>`.
- Produces: One production endpoint using the generated-adapter path while preserving `AxonClient` API.

- [x] **Step 1: Add client-level regression test**

Add this test to `apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientTest.kt` if it does not already exist:

```kotlin
@Test
fun collectionsUsesAuthenticatedV1Route() = runTest {
    server.enqueue(MockResponse().setResponseCode(200).setBody("""{"collections":["axon","docs"]}"""))

    val result = client.collections()

    assertTrue(result.isSuccess)
    assertEquals(listOf("axon", "docs"), result.getOrThrow().collections)
    val request = server.takeRequest()
    assertEquals("/v1/collections", request.path)
    assertEquals("Bearer test-token", request.getHeader("Authorization"))
    assertEquals("test-token", request.getHeader("x-api-key"))
}
```

- [x] **Step 2: Run test before wiring**

Run:

```bash
cd apps/android
./gradlew :app:testDebugUnitTest --tests 'com.axon.app.data.remote.AxonClientTest.collectionsUsesAuthenticatedV1Route'
```

Expected: PASS on current handwritten code. This locks behavior before changing implementation.

- [x] **Step 3: Wire `GeneratedAxonApi` into `AxonClient`**

Modify `apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt`:

```kotlin
private val clients = AxonHttpClients()
private val http = clients.normal
private val httpLong = clients.longRead
private val httpStream = clients.stream
private val generatedApi = GeneratedAxonApi(
    baseUrlProvider = { baseUrl() },
    authProvider = { config.get() },
    clients = clients,
)
```

If `AxonHttpClients` does not exist yet, extract only the existing client construction into `AxonHttpClients` exactly as required by Task 4's tests. Do not refactor stream code in this task.

Replace:

```kotlin
suspend fun collections(): Result<PanelCollectionsResponse> = withContext(Dispatchers.IO) {
    get("/v1/collections")
}
```

with:

```kotlin
suspend fun collections(): Result<PanelCollectionsResponse> = withContext(Dispatchers.IO) {
    generatedApi.collections()
}
```

- [x] **Step 4: Run focused Android tests**

Run:

```bash
cd apps/android
./gradlew :app:openApiGenerate :app:testDebugUnitTest --tests 'com.axon.app.data.remote.GeneratedAxonApiTest' --tests 'com.axon.app.data.remote.AxonClientTest'
```

Expected: PASS.

- [x] **Step 5: Commit**

Run:

```bash
git add apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt apps/android/app/src/main/java/com/axon/app/data/remote/AxonHttpClients.kt apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientTest.kt
git commit -m "refactor: route android collections through generated adapter"
```

Expected: commit succeeds.

---

### Task 6: Document Boundaries And Follow-Up Scope

**Files:**
- Modify: `apps/android/README.md`
- Modify: `docs/superpowers/plans/2026-06-19-android-openapi-generated-client.md`

**Interfaces:**
- Consumes: completed generation and one-endpoint migration.
- Produces: documented workflow and deferred work list.

- [x] **Step 1: Document Android generation workflow**

Append to `apps/android/README.md`:

```markdown
## OpenAPI Client Generation

Android can generate Kotlin client/model code from `../../apps/web/openapi/axon.json`:

```bash
./gradlew :app:openApiGenerate
./gradlew :app:verifyOpenApiGeneratedClient
```

Generated code is written to `app/build/generated/openapi` and is not committed.
Normal JSON REST endpoints may move behind `GeneratedAxonApi` only after
MockWebServer tests prove auth headers, error redaction, and result mapping.

Do not use the generated client for:

- `/api/panel/*` local config/file-write routes
- SSE routes such as `/v1/ask/stream` and `/v1/chat/stream`
- ViewModel/UI-facing DTOs without an explicit repository boundary migration
```
```

- [x] **Step 2: Add follow-up section to this plan**

Append this section to this plan after the task list:

```markdown
## Deferred Follow-Up Plans

- Migrate discovery/read-only family beyond `collections()`.
- Migrate mobile sessions with path-encoding and fail-closed ID tests.
- Migrate job routes with `result_json` and `config_json` preservation tests.
- Delete handwritten DTOs only after all runtime paths are proven generated-backed or intentionally handwritten.
- Extract SSE into `AxonStreamClient` only after generated JSON migration is stable.
```

- [ ] **Step 3: Commit docs**

Run:

```bash
git add apps/android/README.md docs/superpowers/plans/2026-06-19-android-openapi-generated-client.md
git commit -m "docs: document android openapi generation boundaries"
```

Expected: commit succeeds.

---

### Task 7: Final Verification

**Files:**
- No new files.

**Interfaces:**
- Consumes: all previous tasks.
- Produces: green worktree ready for PR and review.

- [ ] **Step 1: Run full relevant verification**

Run:

```bash
cargo xtask check-openapi-drift
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cd apps/android
./gradlew :app:openApiGenerate :app:verifyOpenApiGeneratedClient :app:compileDebugKotlin :app:lintDebug :app:testDebugUnitTest
cd ../..
git diff --check
```

Expected: all commands pass. If `lintDebug` reports pre-existing unrelated issues, fix them inside this worktree because `vibin:work-it` owns the worktree until green.

- [ ] **Step 2: Confirm generated output remains untracked**

Run:

```bash
git status --short -- apps/android/app/build/generated/openapi
```

Expected: no output.

- [ ] **Step 3: Confirm branch status**

Run:

```bash
git status --short --branch
```

Expected: clean branch with local commits.

---

## Failure Mode Checklist

| Codepath | Failure Mode | Rescued? | Test? | User Sees? | Logged? |
|---|---|---:|---:|---|---:|
| Health OpenAPI docs | `/readyz` docs drift from route inventory | Y | Y | Docs mismatch | Y |
| Gradle generation | Wrong spec path or stale generated output | Y | Y | Build/test failure | Y |
| Route/security contract | Protected route marked public | Y | Y | Test failure before ship | Y |
| Forbidden panel routes | Local config route leaks into generated client | Y | Y | Test failure before ship | Y |
| Generated adapter auth | Generated call omits bearer/x-api-key | Y | Y | Test failure before ship | Y |
| Generated adapter errors | Token leaks in exception/log text | Y | Y | Test failure before ship | Y |
| `collections()` migration | Runtime 401 from missing auth | Y | Y | App error if missed | Y |

## Self-Review

**Spec coverage:** The plan still generates Kotlin OpenAPI code, keeps it initially out of production, adds route/security/auth contract tests, lets failures identify OpenAPI or Android drift, and starts replacement with exactly one endpoint.

**Placeholder scan:** The plan contains no `TBD`, `TODO`, or open-ended "add tests" steps. Generator class-name uncertainty is handled by an explicit inspection step before writing generated-backed code.

**Type consistency:** Shared names are consistent: `OpenApiTestPaths`, `AndroidOpenApiRouteContractTest`, `GeneratedOpenApiSmokeTest`, `GeneratedAxonApi`, `GeneratedAxonApiTest`, and `AxonHttpClients`.
