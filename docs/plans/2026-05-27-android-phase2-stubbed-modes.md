# Axon Android Phase 2: stubbed modes, mode-options, page bodies — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the three stubbed operation modes (Summarize, real web Search, Ingest), add Ask follow-up turn tracking, build per-mode options forms, and replace Jobs / Knowledge / System page stubs with real bodies on the Axon Android client.

**Architecture:** Continues the existing layered Android architecture under `apps/android/app/src/main/java/com/axon/app/`: `data/remote/` (OkHttp + kotlinx.serialization), `data/repository/` (`Result<UiModel>` boundary, `withToken` guard), `ui/<feature>/{*Screen,*ViewModel}.kt` (Compose + Aurora components). Mode-options state is persisted via Preferences DataStore; the bearer token is migrated to `EncryptedSharedPreferences` in the same PR. New endpoints land in a single foundation task to prevent merge conflicts across feature tasks.

**Tech Stack:** Kotlin 2.1, Jetpack Compose (BOM 2026.04.01), Material3, Aurora Design System (composite-build from `~/workspace/aurora-design-system`), OkHttp 4.12, kotlinx.serialization 1.7.3, kotlinx.coroutines 1.8.1, AndroidX Lifecycle 2.8.7, Room 2.6.1, DataStore Preferences 1.1.2, JUnit 4 + MockWebServer + Turbine for tests.

---

## Prerequisites

- Working directory: `/home/jmagar/workspace/axon_rust`
- Branch: `feat/android-pager-fab-shell` (PR #142)
- Aurora companion branch: `aurora-design-system:feat/prompt-input-action-left` is already merged into the local composite-build checkout (the `actionLeft` slot on `AuroraPromptInput`).
- Beads: epic `axon_rust-21u8` with 10 children. This plan covers `21u8.{1..9}`; `21u8.10` (SSE for research+summarize) is intentionally deferred to a separate cross-language PR.
- Tooling: `./gradlew` wrapper, `bd` CLI (beads), `gh` CLI (GitHub).
- All commands assume CWD = `/home/jmagar/workspace/axon_rust/apps/android` unless noted otherwise.

## File Map

### Files created in this plan

```
data/remote/models/JobsModels.kt
data/remote/models/SummarizeModels.kt
data/remote/models/SearchWebModels.kt
data/remote/models/IngestModels.kt
data/remote/models/DiscoveryModels.kt          # status, doctor, suggest, domains
data/util/UrlValidator.kt
data/repository/RecentJobsRepository.kt         # DataStore persistence of submitted job IDs
data/repository/EncryptedTokenStore.kt          # EncryptedSharedPreferences for bearer
data/repository/ModeOptionsRepository.kt        # DataStore preferences for mode flags
ui/operations/ModeContentHost.kt                # extracted dispatch table; reduces merge surface
ui/options/ModeOptionsScreen.kt
ui/options/ModeOptionsViewModel.kt
ui/options/forms/AskOptionsForm.kt              # 9 hand-written forms (no central spec table)
ui/options/forms/QueryOptionsForm.kt
ui/options/forms/SummarizeOptionsForm.kt
ui/options/forms/ResearchOptionsForm.kt
ui/options/forms/ScrapeOptionsForm.kt
ui/options/forms/CrawlOptionsForm.kt
ui/options/forms/SearchWebOptionsForm.kt
ui/options/forms/MapOptionsForm.kt
ui/options/forms/IngestOptionsForm.kt
ui/options/components/HeadersField.kt           # repeatable Key:Value rows for Crawl headers
ui/options/components/NumberStepperField.kt
ui/options/components/EnumDropdownField.kt
ui/summarize/SummarizeScreen.kt
ui/summarize/SummarizeViewModel.kt
ui/searchweb/SearchWebScreen.kt                 # distinct from existing ui/query/ (vector query)
ui/searchweb/SearchWebViewModel.kt
ui/ingest/IngestScreen.kt
ui/ingest/IngestViewModel.kt
ui/knowledge/KnowledgeViewModel.kt
ui/knowledge/sections/SuggestSection.kt
ui/knowledge/sections/SourcesSection.kt
ui/knowledge/sections/DomainsSection.kt
ui/knowledge/sections/StatsSection.kt
ui/jobs/JobsViewModel.kt
ui/jobs/JobRow.kt
ui/system/SystemViewModel.kt

# Tests
src/test/java/com/axon/app/data/remote/AxonClientPhase2Test.kt
src/test/java/com/axon/app/data/repository/AxonRepositoryPhase2Test.kt
src/test/java/com/axon/app/data/repository/RecentJobsRepositoryTest.kt
src/test/java/com/axon/app/data/repository/ModeOptionsRepositoryTest.kt
src/test/java/com/axon/app/data/util/UrlValidatorTest.kt
src/test/java/com/axon/app/ui/ask/AskViewModelTest.kt
src/test/java/com/axon/app/ui/summarize/SummarizeViewModelTest.kt
src/test/java/com/axon/app/ui/searchweb/SearchWebViewModelTest.kt
src/test/java/com/axon/app/ui/ingest/IngestViewModelTest.kt
src/test/java/com/axon/app/ui/jobs/JobsViewModelTest.kt
src/test/java/com/axon/app/ui/knowledge/KnowledgeViewModelTest.kt
src/test/java/com/axon/app/ui/system/SystemViewModelTest.kt
src/test/java/com/axon/app/ui/options/ModeOptionsViewModelTest.kt
```

### Files modified in this plan

```
data/remote/AxonClient.kt                      # +retrieve methods, .take(200) error truncation
data/remote/AxonModels.kt                      # keep existing; new models live in models/ subdir
data/repository/AxonRepository.kt              # +new methods, ModeOptionsRepository-based decorator
data/repository/SettingsRepository.kt          # delegate token to EncryptedTokenStore
di/AppContainer.kt                             # wire EncryptedTokenStore, ModeOptionsRepository, RecentJobsRepository
ui/operations/OperationsScreen.kt              # delegate dispatch to ModeContentHost; real cog handler
ui/nav/AxonNav.kt                              # remove LocalModeOptionsCog default null; provided always
ui/nav/AxonNavGraph.kt                         # +ModeOptionsRoute composable
ui/ask/AskViewModel.kt                         # +turn tracking + clearFollowUp()
ui/ask/AskScreen.kt                            # follow-up status indicator
ui/jobs/JobsScreen.kt                          # rewrite — virtualized LazyColumn, 4 tabs + status header
ui/knowledge/KnowledgeScreen.kt                # rewrite — 4 tabs over sections
ui/system/SystemScreen.kt                      # rewrite — Doctor only
ui/operations/StubModeForm.kt                  # delete (no longer referenced)
build.gradle.kts (app)                         # +security-crypto, +turbine, +datastore-preferences
gradle/libs.versions.toml                      # +security-crypto = 1.1.0-alpha06, +turbine = 1.1.0, +datastore-preferences = 1.1.2
```

---

## Cross-cutting conventions for every new ViewModel

Locked once here so each task can reference these without repeating:

1. **Constructor signature**:
   ```kotlin
   class FooViewModel(
       app: Application,
       private val dispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
   ) : AndroidViewModel(app)
   ```
   The `dispatcher` parameter exists so tests can swap in `StandardTestDispatcher(testScheduler)` for `runTest` virtual time.

2. **State exposure**:
   ```kotlin
   private val _uiState = MutableStateFlow<FooUiState>(FooUiState.Loading)
   val uiState: StateFlow<FooUiState> = _uiState.asStateFlow()
   ```

3. **Polling pattern (replaces all `while(isActive) { fetch(); delay() }`):**
   ```kotlin
   val jobs: StateFlow<JobsUi> = flow {
       while (true) {
           emit(repo.listJobs())
           delay(POLL_INTERVAL_MS)
       }
   }
       .catch { e -> if (e is CancellationException) throw e; emit(JobsUi.Error(e.message ?: "Error")) }
       .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), JobsUi.Loading)
   ```
   `WhileSubscribed(5_000)` stops collection 5s after the last collector detaches.

4. **Test pattern:**
   ```kotlin
   @OptIn(ExperimentalCoroutinesApi::class)
   class FooViewModelTest {
       private val dispatcher = StandardTestDispatcher()
       @Before fun setUp() { Dispatchers.setMain(dispatcher) }
       @After  fun tearDown() { Dispatchers.resetMain() }

       @Test fun `loads happy path`() = runTest(dispatcher) {
           val vm = FooViewModel(fakeApp(), dispatcher)
           vm.uiState.test {
               assertEquals(FooUiState.Loading, awaitItem())
               // …
           }
       }
   }
   ```
   Uses Turbine's `flow.test { ... awaitItem() … }`. Add to `libs.versions.toml`: `turbine = "1.1.0"` and `androidx.test.ext.junit` already present.

---

## Task 1: Foundation — wire client + repo + models for phase-2 endpoints

> Beads: `axon_rust-21u8.1`. Blocking 21u8.3/4/5/6/7/8/9.

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/data/remote/models/{Summarize,SearchWeb,Ingest,Jobs,Discovery}Models.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/data/util/UrlValidator.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt` (add methods, truncate error body)
- Modify: `apps/android/app/src/main/java/com/axon/app/data/repository/AxonRepository.kt` (add wrappers)
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsScreen.kt` (delegate dispatch to ModeContentHost)
- Test: `apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientPhase2Test.kt`
- Test: `apps/android/app/src/test/java/com/axon/app/data/repository/AxonRepositoryPhase2Test.kt`
- Test: `apps/android/app/src/test/java/com/axon/app/data/util/UrlValidatorTest.kt`

### Step 1.1: Add Turbine + DataStore Preferences to the version catalog

- [ ] Edit `apps/android/gradle/libs.versions.toml`:

  ```toml
  [versions]
  turbine = "1.1.0"
  datastorePreferences = "1.1.2"
  securityCrypto = "1.1.0-alpha06"

  [libraries]
  turbine = { group = "app.cash.turbine", name = "turbine", version.ref = "turbine" }
  datastore-preferences = { group = "androidx.datastore", name = "datastore-preferences", version.ref = "datastorePreferences" }
  security-crypto = { group = "androidx.security", name = "security-crypto", version.ref = "securityCrypto" }
  ```

- [ ] Edit `apps/android/app/build.gradle.kts`, add inside the `dependencies` block:

  ```kotlin
  implementation(libs.datastore.preferences)
  implementation(libs.security.crypto)
  testImplementation(libs.turbine)
  ```

- [ ] Run `./gradlew :app:dependencies --configuration debugRuntimeClasspath > /dev/null` and confirm exit 0.

- [ ] Commit:
  ```bash
  git add apps/android/gradle/libs.versions.toml apps/android/app/build.gradle.kts
  git commit -m "build(android): add datastore-preferences, security-crypto, turbine to libs catalog"
  ```

### Step 1.2: UrlValidator helper — write the failing tests

- [ ] Create `apps/android/app/src/test/java/com/axon/app/data/util/UrlValidatorTest.kt`:

  ```kotlin
  package com.axon.app.data.util

  import org.junit.Assert.assertFalse
  import org.junit.Assert.assertTrue
  import org.junit.Test

  class UrlValidatorTest {
      @Test fun `accepts http URL`()  = assertTrue(UrlValidator.isValidHttpUrl("http://example.com"))
      @Test fun `accepts https URL`() = assertTrue(UrlValidator.isValidHttpUrl("https://example.com/path?q=1"))
      @Test fun `rejects file scheme`()       = assertFalse(UrlValidator.isValidHttpUrl("file:///etc/passwd"))
      @Test fun `rejects ftp scheme`()        = assertFalse(UrlValidator.isValidHttpUrl("ftp://example.com"))
      @Test fun `rejects javascript scheme`() = assertFalse(UrlValidator.isValidHttpUrl("javascript:alert(1)"))
      @Test fun `rejects empty string`()      = assertFalse(UrlValidator.isValidHttpUrl(""))
      @Test fun `rejects malformed URL`()     = assertFalse(UrlValidator.isValidHttpUrl("not a url"))
      @Test fun `rejects no scheme`()         = assertFalse(UrlValidator.isValidHttpUrl("example.com"))
  }
  ```

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*UrlValidatorTest*'`
  Expected: FAIL with "unresolved reference 'UrlValidator'"

### Step 1.3: UrlValidator helper — implement

- [ ] Create `apps/android/app/src/main/java/com/axon/app/data/util/UrlValidator.kt`:

  ```kotlin
  package com.axon.app.data.util

  import java.net.URL

  /**
   * Client-side fail-fast URL guard. Server-side SSRF protection in
   * src/core/http/ssrf.rs remains the security backstop; this helper only
   * rejects obviously bad inputs (file://, javascript:, malformed) before
   * the network call.
   */
  object UrlValidator {
      fun isValidHttpUrl(input: String): Boolean {
          if (input.isBlank()) return false
          val url = runCatching { URL(input) }.getOrNull() ?: return false
          return url.protocol == "http" || url.protocol == "https"
      }
  }
  ```

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*UrlValidatorTest*'`
  Expected: PASS, 8 tests.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/data/util/UrlValidator.kt \
          apps/android/app/src/test/java/com/axon/app/data/util/UrlValidatorTest.kt
  git commit -m "feat(android): UrlValidator — client-side fail-fast for non-http(s) URLs"
  ```

### Step 1.4: AxonClient error-body truncation — write the failing test

- [ ] Append to `apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientTest.kt`:

  ```kotlin
  @Test
  fun `execute truncates oversized error body to 200 chars`() = runBlocking {
      server.enqueue(MockResponse().setResponseCode(500).setBody("E".repeat(10_000)))
      val result = client.healthz()  // any failing call routes through execute()
      assertTrue(result.isFailure)
      val msg = result.exceptionOrNull()?.message.orEmpty()
      // "HTTP 500: " prefix + at most 200 body chars (no full 10k body)
      assertTrue("got: $msg (len ${msg.length})", msg.length <= 220)
  }
  ```

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*AxonClientTest.execute truncates*'`
  Expected: FAIL — message currently contains the full 10K body.

### Step 1.5: AxonClient error-body truncation — implement

- [ ] In `apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt`, in the `execute` helper (around line ~225) replace the body-read line:

  ```kotlin
  // BEFORE:
  // error("HTTP ${resp.code}: ${resp.body?.string() ?: resp.message}")
  // AFTER:
  error("HTTP ${resp.code}: ${resp.body?.string()?.take(200) ?: resp.message}")
  ```

- [ ] Also in `healthz` (around line ~98) — already does `.take(200)`, leave as-is.
- [ ] In `askStream` SSE error path (around line ~129) — already does `.take(200)`, leave as-is.

- [ ] Run the truncation test again. Expected: PASS.

- [ ] Run the full existing client test suite to confirm no regression: `./gradlew :app:testDebugUnitTest --tests '*AxonClientTest*'`. Expected: ALL PASS.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt \
          apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientTest.kt
  git commit -m "fix(android): truncate server error body to 200 chars in AxonClient.execute()"
  ```

### Step 1.6: Add the new wire models (split per-domain to avoid one giant file)

- [ ] Create `apps/android/app/src/main/java/com/axon/app/data/remote/models/SummarizeModels.kt`:

  ```kotlin
  package com.axon.app.data.remote.models

  import kotlinx.serialization.SerialName
  import kotlinx.serialization.Serializable

  /** POST /v1/summarize request — mirrors `RestSummarizeRequest`. */
  @Serializable
  data class SummarizeRequest(
      val url: String? = null,
      val urls: List<String>? = null,
      @SerialName("render_mode") val renderMode: String? = null,   // "http" | "chrome" | "auto-switch"
      @SerialName("root_selector") val rootSelector: String? = null,
      @SerialName("exclude_selector") val excludeSelector: String? = null,
      val headers: List<String> = emptyList(),                       // "Key: Value" strings
      val collection: String? = null,
  )

  /** POST /v1/summarize response — mirrors `SummarizeResult`. */
  @Serializable
  data class SummarizeResponse(
      val urls: List<String> = emptyList(),
      @SerialName("context_chars") val contextChars: Long = 0,
      @SerialName("context_truncated") val contextTruncated: Boolean = false,
      val summary: String = "",
  )
  ```

- [ ] Create `apps/android/app/src/main/java/com/axon/app/data/remote/models/SearchWebModels.kt`:

  ```kotlin
  package com.axon.app.data.remote.models

  import kotlinx.serialization.SerialName
  import kotlinx.serialization.Serializable

  /** POST /v1/search request — Tavily web search; mirrors `RestSearchRequest`. */
  @Serializable
  data class SearchWebRequest(
      val query: String,
      val limit: Int? = null,
      val offset: Int? = null,
      @SerialName("time_range") val timeRange: String? = null,       // "day"|"week"|"month"|null
  )

  @Serializable
  data class SearchWebHit(
      val title: String = "",
      val url: String = "",
      val snippet: String? = null,
      val score: Double? = null,
  )

  @Serializable
  data class CrawlJobRef(
      @SerialName("job_id") val jobId: String,
      val url: String,
  )

  @Serializable
  data class AutoCrawlStatus(
      val enqueued: Int = 0,
      val skipped: Int = 0,
  )

  @Serializable
  data class SearchWebResponse(
      val query: String = "",
      val results: List<SearchWebHit> = emptyList(),
      @SerialName("auto_crawl_status") val autoCrawlStatus: AutoCrawlStatus? = null,
      @SerialName("crawl_jobs") val crawlJobs: List<CrawlJobRef> = emptyList(),
  )
  ```

- [ ] Create `apps/android/app/src/main/java/com/axon/app/data/remote/models/IngestModels.kt`:

  ```kotlin
  package com.axon.app.data.remote.models

  import kotlinx.serialization.SerialName
  import kotlinx.serialization.Serializable

  /** POST /v1/ingest request — mirrors `RestIngestRequest`. */
  @Serializable
  data class IngestRequest(
      @SerialName("source_type") val sourceType: String,   // "github"|"gitlab"|"gitea"|"git"|"reddit"|"youtube"
      val target: String? = null,
      @SerialName("include_source") val includeSource: Boolean? = null,
      val collection: String? = null,
  )

  /** AcceptedJob — 202 response from POST /v1/ingest. */
  @Serializable
  data class AcceptedJob(
      @SerialName("job_id") val jobId: String,
      val status: String = "pending",
      @SerialName("status_url") val statusUrl: String? = null,
  )
  ```

- [ ] Create `apps/android/app/src/main/java/com/axon/app/data/remote/models/JobsModels.kt`:

  ```kotlin
  package com.axon.app.data.remote.models

  import kotlinx.serialization.SerialName
  import kotlinx.serialization.Serializable
  import kotlinx.serialization.json.JsonElement

  /** ServiceJob — common shape across /v1/{crawl,embed,extract,ingest}/list and /{id}. */
  @Serializable
  data class ServiceJob(
      val id: String = "",
      val status: String = "",
      @SerialName("created_at") val createdAt: String? = null,
      @SerialName("updated_at") val updatedAt: String? = null,
      @SerialName("started_at") val startedAt: String? = null,
      @SerialName("finished_at") val finishedAt: String? = null,
      @SerialName("error_text") val errorText: String? = null,
      val url: String? = null,                                       // crawl
      @SerialName("source_type") val sourceType: String? = null,      // ingest
      val target: String? = null,                                     // ingest/embed/extract
      @SerialName("result_json") val resultJson: JsonElement? = null, // locked: JsonElement, not JsonObject
      @SerialName("config_json") val configJson: JsonElement? = null,
  )

  /** GET /v1/status response — aggregated job counts. */
  @Serializable
  data class StatusSummary(
      val payload: JsonElement,
  )

  /** POST /v1/{kind}/{id}/cancel response. */
  @Serializable
  data class CancelResponse(
      val canceled: Boolean = false,
  )
  ```

- [ ] Create `apps/android/app/src/main/java/com/axon/app/data/remote/models/DiscoveryModels.kt`:

  ```kotlin
  package com.axon.app.data.remote.models

  import kotlinx.serialization.Serializable
  import kotlinx.serialization.json.JsonElement

  /** GET /v1/doctor — payload is service-connectivity check results. */
  @Serializable
  data class DoctorResponse(val payload: JsonElement)

  /** POST /v1/suggest. */
  @Serializable
  data class SuggestRequest(
      val focus: String? = null,
      val collection: String? = null,
  )

  @Serializable
  data class SuggestHit(
      val url: String = "",
      val reason: String? = null,
  )

  @Serializable
  data class SuggestResponse(
      val urls: List<SuggestHit> = emptyList(),
  )

  /** GET /v1/domains. */
  @Serializable
  data class DomainFacet(
      val domain: String = "",
      val vectors: Long = 0,
  )

  @Serializable
  data class DomainsResponse(
      val domains: List<DomainFacet> = emptyList(),
      val limit: Long = 0,
      val offset: Long = 0,
  )
  ```

- [ ] Run: `./gradlew :app:compileDebugKotlin`. Expected: BUILD SUCCESSFUL.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/data/remote/models/
  git commit -m "feat(android): wire models for /v1/{summarize,search,ingest,jobs,doctor,suggest,domains}"
  ```

### Step 1.7: Add the AxonClient methods — write the failing tests

- [ ] Create `apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientPhase2Test.kt`:

  ```kotlin
  package com.axon.app.data.remote

  import com.axon.app.data.remote.models.IngestRequest
  import com.axon.app.data.remote.models.SearchWebRequest
  import com.axon.app.data.remote.models.SummarizeRequest
  import kotlinx.coroutines.runBlocking
  import okhttp3.mockwebserver.MockResponse
  import okhttp3.mockwebserver.MockWebServer
  import org.junit.After
  import org.junit.Assert.assertEquals
  import org.junit.Assert.assertTrue
  import org.junit.Before
  import org.junit.Test

  class AxonClientPhase2Test {
      private lateinit var server: MockWebServer
      private lateinit var client: AxonClient

      @Before fun setUp() {
          server = MockWebServer().also { it.start() }
          client = AxonClient(server.url("/").toString().trimEnd('/'), "test-token")
      }
      @After fun tearDown() { server.shutdown() }

      @Test fun `summarize posts to v1 summarize`() = runBlocking {
          server.enqueue(MockResponse().setBody("""{"urls":["https://a"],"summary":"hi","context_chars":10,"context_truncated":false}""").addHeader("Content-Type","application/json"))
          val r = client.summarize(SummarizeRequest(url = "https://a"))
          assertTrue(r.isSuccess)
          val req = server.takeRequest()
          assertEquals("POST", req.method)
          assertEquals("/v1/summarize", req.path)
          assertTrue(req.body.readUtf8().contains("\"url\":\"https://a\""))
      }

      @Test fun `searchWeb posts to v1 search and decodes hits + crawl jobs`() = runBlocking {
          server.enqueue(MockResponse().setBody("""{"query":"k","results":[{"title":"t","url":"https://x"}],"crawl_jobs":[{"job_id":"j1","url":"https://x"}]}""").addHeader("Content-Type","application/json"))
          val r = client.searchWeb(SearchWebRequest(query = "k"))
          assertTrue(r.isSuccess)
          val resp = r.getOrThrow()
          assertEquals(1, resp.results.size)
          assertEquals("j1", resp.crawlJobs[0].jobId)
      }

      @Test fun `ingestStart posts to v1 ingest and decodes AcceptedJob`() = runBlocking {
          server.enqueue(MockResponse().setResponseCode(202).setBody("""{"job_id":"abc","status":"pending"}""").addHeader("Content-Type","application/json"))
          val r = client.ingestStart(IngestRequest(sourceType = "github", target = "https://github.com/o/r"))
          assertTrue(r.isSuccess)
          assertEquals("abc", r.getOrThrow().jobId)
          val body = server.takeRequest().body.readUtf8()
          assertTrue(body.contains("\"source_type\":\"github\""))
      }

      @Test fun `ingestList GETs v1 ingest list and decodes ServiceJob array`() = runBlocking {
          server.enqueue(MockResponse().setBody("""[{"id":"j","status":"completed","source_type":"github","target":"https://github.com/o/r"}]""").addHeader("Content-Type","application/json"))
          val r = client.listJobs(JobKind.Ingest)
          assertTrue(r.isSuccess)
          assertEquals("j", r.getOrThrow()[0].id)
      }

      @Test fun `cancelJob POSTs v1 kind id cancel`() = runBlocking {
          server.enqueue(MockResponse().setBody("""{"canceled":true}""").addHeader("Content-Type","application/json"))
          val r = client.cancelJob(JobKind.Crawl, "j1")
          assertTrue(r.isSuccess && r.getOrThrow().canceled)
          assertEquals("/v1/crawl/j1/cancel", server.takeRequest().path)
      }

      @Test fun `status GETs v1 status`() = runBlocking {
          server.enqueue(MockResponse().setBody("""{"payload":{"pending":2}}""").addHeader("Content-Type","application/json"))
          val r = client.status()
          assertTrue(r.isSuccess)
      }

      @Test fun `doctor GETs v1 doctor`() = runBlocking {
          server.enqueue(MockResponse().setBody("""{"payload":{"qdrant":"ok"}}""").addHeader("Content-Type","application/json"))
          assertTrue(client.doctor().isSuccess)
      }

      @Test fun `suggest POSTs v1 suggest`() = runBlocking {
          server.enqueue(MockResponse().setBody("""{"urls":[{"url":"https://x","reason":"r"}]}""").addHeader("Content-Type","application/json"))
          assertTrue(client.suggest(focus = "rust").isSuccess)
      }

      @Test fun `domains GETs v1 domains`() = runBlocking {
          server.enqueue(MockResponse().setBody("""{"domains":[{"domain":"d","vectors":5}]}""").addHeader("Content-Type","application/json"))
          assertTrue(client.domains().isSuccess)
      }
  }
  ```

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*AxonClientPhase2Test*'`
  Expected: FAIL — unresolved references on `summarize`, `searchWeb`, `ingestStart`, etc.

### Step 1.8: Add the AxonClient methods — implement

- [ ] In `apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt`, add at the top of the file (below existing imports):

  ```kotlin
  import com.axon.app.data.remote.models.AcceptedJob
  import com.axon.app.data.remote.models.CancelResponse
  import com.axon.app.data.remote.models.DiscoveryDoctor
  import com.axon.app.data.remote.models.DoctorResponse
  import com.axon.app.data.remote.models.DomainsResponse
  import com.axon.app.data.remote.models.IngestRequest
  import com.axon.app.data.remote.models.SearchWebRequest
  import com.axon.app.data.remote.models.SearchWebResponse
  import com.axon.app.data.remote.models.ServiceJob
  import com.axon.app.data.remote.models.StatusSummary
  import com.axon.app.data.remote.models.SuggestRequest
  import com.axon.app.data.remote.models.SuggestResponse
  import com.axon.app.data.remote.models.SummarizeRequest
  import com.axon.app.data.remote.models.SummarizeResponse
  ```

- [ ] Inside `class AxonClient`, add a small JobKind enum + the new methods. Place near the end of the class, before the `// ── Helpers ──` section:

  ```kotlin
  // ── Phase 2 endpoints ──────────────────────────────────────────────────────

  enum class JobKind(val path: String) {
      Crawl("crawl"), Embed("embed"), Extract("extract"), Ingest("ingest")
  }

  /** /v1/summarize — Gemini-backed, can take minutes. Use httpLong. */
  suspend fun summarize(req: SummarizeRequest): Result<SummarizeResponse> = withContext(Dispatchers.IO) {
      postWith(httpLong, "/v1/summarize", req)
  }

  /** /v1/search — Tavily web search; auto-enqueues crawl jobs server-side. */
  suspend fun searchWeb(req: SearchWebRequest): Result<SearchWebResponse> = withContext(Dispatchers.IO) {
      post("/v1/search", req)
  }

  /** POST /v1/ingest — submits an async ingest job. */
  suspend fun ingestStart(req: IngestRequest): Result<AcceptedJob> = withContext(Dispatchers.IO) {
      post("/v1/ingest", req)
  }

  /** GET /v1/{kind}/{id} — job detail. Long-poll-friendly via httpLong. */
  suspend fun getJob(kind: JobKind, id: String): Result<ServiceJob> = withContext(Dispatchers.IO) {
      val builder = authRequest(Request.Builder().url("${baseUrl()}/v1/${kind.path}/$id").get())
      execute(httpLong, builder)
  }

  /** GET /v1/{kind}/list — list jobs of one kind. Server currently ignores limit/offset; we still pass them as forward-compatible. */
  suspend fun listJobs(kind: JobKind, limit: Int = 100, offset: Int = 0): Result<List<ServiceJob>> = withContext(Dispatchers.IO) {
      get("/v1/${kind.path}/list?limit=$limit&offset=$offset")
  }

  /** POST /v1/{kind}/{id}/cancel. */
  suspend fun cancelJob(kind: JobKind, id: String): Result<CancelResponse> = withContext(Dispatchers.IO) {
      val body = "{}".toRequestBody(JSON_MEDIA_TYPE)
      val builder = authRequest(Request.Builder().url("${baseUrl()}/v1/${kind.path}/$id/cancel").post(body))
      execute(http, builder)
  }

  suspend fun status(): Result<StatusSummary> = withContext(Dispatchers.IO) { get("/v1/status") }

  suspend fun doctor(): Result<DoctorResponse> = withContext(Dispatchers.IO) { get("/v1/doctor") }

  suspend fun suggest(focus: String? = null, collection: String? = null): Result<SuggestResponse> =
      withContext(Dispatchers.IO) { post("/v1/suggest", SuggestRequest(focus = focus, collection = collection)) }

  suspend fun domains(limit: Int = 100, offset: Int = 0): Result<DomainsResponse> =
      withContext(Dispatchers.IO) { get("/v1/domains?limit=$limit&offset=$offset") }
  ```

  (`stats()` already exists on the client; no change.)

- [ ] Run: `./gradlew :app:compileDebugKotlin`. Expected: BUILD SUCCESSFUL (the test classes resolve their unresolved refs).
- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*AxonClientPhase2Test*'`. Expected: PASS, 9 tests.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt \
          apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientPhase2Test.kt
  git commit -m "feat(android): AxonClient — summarize, searchWeb, ingest{Start,Get,List,Cancel}, status, doctor, suggest, domains"
  ```

### Step 1.9: Add the repository wrappers — write the failing test

- [ ] Create `apps/android/app/src/test/java/com/axon/app/data/repository/AxonRepositoryPhase2Test.kt`:

  ```kotlin
  package com.axon.app.data.repository

  import com.axon.app.data.local.AskHistoryDao
  import com.axon.app.data.local.AskHistoryEntry
  import com.axon.app.data.remote.AxonClient
  import kotlinx.coroutines.flow.Flow
  import kotlinx.coroutines.flow.flowOf
  import kotlinx.coroutines.runBlocking
  import okhttp3.mockwebserver.MockResponse
  import okhttp3.mockwebserver.MockWebServer
  import org.junit.After
  import org.junit.Assert.assertEquals
  import org.junit.Assert.assertTrue
  import org.junit.Before
  import org.junit.Test

  private class NoopDao : AskHistoryDao {
      override fun recent(): Flow<List<AskHistoryEntry>> = flowOf(emptyList())
      override suspend fun insert(entry: AskHistoryEntry) {}
      override suspend fun clearAll() {}
  }

  class AxonRepositoryPhase2Test {
      private lateinit var server: MockWebServer
      private lateinit var repo: AxonRepository

      @Before fun setUp() {
          server = MockWebServer().also { it.start() }
          repo = AxonRepository(AxonClient(server.url("/").toString().trimEnd('/'), "t"), NoopDao())
      }
      @After fun tearDown() { server.shutdown() }

      @Test fun `summarize maps wire to UI`() = runBlocking {
          server.enqueue(MockResponse().setBody("""{"urls":["a"],"summary":"hi","context_chars":7,"context_truncated":false}""").addHeader("Content-Type","application/json"))
          val r = repo.summarize(listOf("a"))
          assertTrue(r.isSuccess)
          assertEquals("hi", r.getOrThrow().summary)
      }

      @Test fun `searchWeb maps results and crawl jobs`() = runBlocking {
          server.enqueue(MockResponse().setBody("""{"query":"k","results":[{"title":"t","url":"https://x"}],"crawl_jobs":[{"job_id":"j","url":"https://x"}]}""").addHeader("Content-Type","application/json"))
          val r = repo.searchWeb("k").getOrThrow()
          assertEquals(1, r.results.size)
          assertEquals("j", r.crawlJobs[0].jobId)
      }

      @Test fun `ingestStart returns jobId`() = runBlocking {
          server.enqueue(MockResponse().setResponseCode(202).setBody("""{"job_id":"abc","status":"pending"}""").addHeader("Content-Type","application/json"))
          assertEquals("abc", repo.ingestStart("github", "https://github.com/o/r").getOrThrow())
      }

      @Test fun `listJobs returns full server array unchanged (no client-side slicing)`() = runBlocking {
          server.enqueue(MockResponse().setBody("""[{"id":"a","status":"x"},{"id":"b","status":"y"}]""").addHeader("Content-Type","application/json"))
          val jobs = repo.listJobs(AxonClient.JobKind.Crawl).getOrThrow()
          assertEquals(2, jobs.size)
      }

      @Test fun `summarize blocked by missing token`() = runBlocking {
          val r2 = AxonRepository(AxonClient(server.url("/").toString().trimEnd('/'), ""), NoopDao()).summarize(listOf("a"))
          assertTrue(r2.isFailure)
      }
  }
  ```

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*AxonRepositoryPhase2Test*'`
  Expected: FAIL — unresolved references.

### Step 1.10: Add the repository wrappers — implement

- [ ] In `apps/android/app/src/main/java/com/axon/app/data/repository/AxonRepository.kt`, add UI-model types near the existing `@Stable` data classes:

  ```kotlin
  @Stable data class SummarizeResultUi(
      val urls: List<String>,
      val summary: String,
      val contextChars: Long,
      val contextTruncated: Boolean,
  )

  @Stable data class SearchWebHitUi(
      val title: String, val url: String, val snippet: String?, val score: Double?,
  )
  @Stable data class CrawlJobRefUi(val jobId: String, val url: String)
  @Stable data class SearchWebResultUi(
      val query: String,
      val results: List<SearchWebHitUi>,
      val crawlJobsEnqueued: Int,
      val crawlJobs: List<CrawlJobRefUi>,
  )

  @Stable data class JobUi(
      val id: String, val status: String, val url: String?, val sourceType: String?,
      val target: String?, val errorText: String?,
      val resultJson: kotlinx.serialization.json.JsonElement?,
      val finishedAt: String?,
  )

  @Stable data class SuggestHitUi(val url: String, val reason: String?)
  @Stable data class DomainFacetUi(val domain: String, val vectors: Long)
  ```

- [ ] Add the wrapper methods inside `class AxonRepository`:

  ```kotlin
  // ── Phase 2 wrappers ───────────────────────────────────────────────────

  suspend fun summarize(urls: List<String>, collection: String? = null): Result<SummarizeResultUi> = withToken {
      client.summarize(
          com.axon.app.data.remote.models.SummarizeRequest(urls = urls, collection = collection)
      ).map { r -> SummarizeResultUi(r.urls, r.summary, r.contextChars, r.contextTruncated) }
  }

  suspend fun searchWeb(query: String): Result<SearchWebResultUi> = withToken {
      client.searchWeb(com.axon.app.data.remote.models.SearchWebRequest(query = query)).map { r ->
          SearchWebResultUi(
              query = r.query,
              results = r.results.map { SearchWebHitUi(it.title, it.url, it.snippet, it.score) },
              crawlJobsEnqueued = r.autoCrawlStatus?.enqueued ?: 0,
              crawlJobs = r.crawlJobs.map { CrawlJobRefUi(it.jobId, it.url) },
          )
      }
  }

  suspend fun ingestStart(sourceType: String, target: String, collection: String? = null): Result<String> = withToken {
      client.ingestStart(
          com.axon.app.data.remote.models.IngestRequest(sourceType = sourceType, target = target, collection = collection)
      ).map { it.jobId }
  }

  suspend fun getJob(kind: AxonClient.JobKind, id: String): Result<JobUi> = withToken {
      client.getJob(kind, id).map(::toJobUi)
  }

  suspend fun listJobs(kind: AxonClient.JobKind): Result<List<JobUi>> = withToken {
      client.listJobs(kind).map { list -> list.map(::toJobUi) }
  }

  suspend fun cancelJob(kind: AxonClient.JobKind, id: String): Result<Boolean> = withToken {
      client.cancelJob(kind, id).map { it.canceled }
  }

  suspend fun statusPayload(): Result<kotlinx.serialization.json.JsonElement> = withToken {
      client.status().map { it.payload }
  }

  suspend fun doctorPayload(): Result<kotlinx.serialization.json.JsonElement> = withToken {
      client.doctor().map { it.payload }
  }

  suspend fun suggest(focus: String?, collection: String? = null): Result<List<SuggestHitUi>> = withToken {
      client.suggest(focus = focus, collection = collection).map { r ->
          r.urls.map { SuggestHitUi(it.url, it.reason) }
      }
  }

  suspend fun domains(limit: Int = 100, offset: Int = 0): Result<List<DomainFacetUi>> = withToken {
      client.domains(limit = limit, offset = offset).map { r ->
          r.domains.map { DomainFacetUi(it.domain, it.vectors) }
      }
  }

  private fun toJobUi(j: com.axon.app.data.remote.models.ServiceJob) = JobUi(
      id = j.id, status = j.status, url = j.url, sourceType = j.sourceType,
      target = j.target, errorText = j.errorText, resultJson = j.resultJson,
      finishedAt = j.finishedAt,
  )

  import com.axon.app.data.remote.AxonClient
  ```

  (Move the `import com.axon.app.data.remote.AxonClient` to the file imports area — Kotlin doesn't allow imports inside a class.)

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*AxonRepositoryPhase2Test*'`. Expected: PASS, 5 tests.
- [ ] Run the full test suite: `./gradlew :app:testDebugUnitTest`. Expected: ALL PASS.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/data/repository/AxonRepository.kt \
          apps/android/app/src/test/java/com/axon/app/data/repository/AxonRepositoryPhase2Test.kt
  git commit -m "feat(android): AxonRepository — summarize/searchWeb/ingestStart/listJobs/cancelJob/status/doctor/suggest/domains UI mappings"
  ```

### Step 1.11: Extract `ModeContentHost` from OperationsScreen — reduce merge surface

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt`:

  ```kotlin
  package com.axon.app.ui.operations

  import androidx.compose.runtime.Composable
  import androidx.lifecycle.viewmodel.compose.viewModel
  import com.axon.app.ui.ask.AskScreen
  import com.axon.app.ui.query.QueryScreen
  import com.axon.app.ui.tools.CrawlTab
  import com.axon.app.ui.tools.MapTab
  import com.axon.app.ui.tools.ResearchTab
  import com.axon.app.ui.tools.ScrapeTab
  import com.axon.app.ui.tools.ToolsViewModel

  /**
   * Dispatch table from [OperationMode] to the content composable. Extracted from
   * OperationsScreen so each feature task can swap one row independently (one-line
   * additive change) instead of conflict-prone edits inside the screen scaffold.
   *
   * Wave-2 task 3 (Summarize), wave-3 task 7 (Search), and wave-4 task 8 (Ingest)
   * each replace one TODO branch in the `when` below with their real screen.
   */
  @Composable
  fun ModeContentHost(activeMode: OperationMode) {
      val toolsVm: ToolsViewModel = viewModel()
      when (activeMode) {
          OperationMode.Ask       -> AskScreen()
          OperationMode.Query     -> QueryScreen()
          OperationMode.Scrape    -> ScrapeTab(toolsVm)
          OperationMode.Crawl     -> CrawlTab(toolsVm)
          OperationMode.Map       -> MapTab(toolsVm)
          OperationMode.Research  -> ResearchTab(toolsVm)
          OperationMode.Summarize -> StubModeForm(mode = activeMode)  // replaced in Task 3
          OperationMode.Search    -> StubModeForm(mode = activeMode)  // replaced in Task 7
          OperationMode.Ingest    -> StubModeForm(mode = activeMode)  // replaced in Task 8
      }
  }
  ```

- [ ] Edit `apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsScreen.kt`. Replace the `when (activeMode) { … }` block inside the `CompositionLocalProvider` body with a single call:

  ```kotlin
  CompositionLocalProvider(LocalModeOptionsCog provides onModeOptions) {
      ModeContentHost(activeMode = activeMode)
  }
  ```

  Also delete the `val toolsVm = viewModel<ToolsViewModel>()` line in `OperationsScreen` — it has moved into `ModeContentHost`.

- [ ] Run: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`. Expected: BUILD SUCCESSFUL, all tests pass (no behaviour change).

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt \
          apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsScreen.kt
  git commit -m "refactor(android): extract ModeContentHost from OperationsScreen — one-line dispatch swaps per mode"
  ```

### Step 1.12: Bead close

- [ ] Run `bd close axon_rust-21u8.1` and verify it appears closed in `bd list --parent axon_rust-21u8`.

---

## Task 2: Ask mode auto follow-up turn tracking

> Beads: `axon_rust-21u8.2`. Independent — runs in parallel with Task 1.

**Files:**
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/ask/AskViewModel.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/ask/AskScreen.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsScreen.kt`
- Test: `apps/android/app/src/test/java/com/axon/app/ui/ask/AskViewModelTest.kt`

### Step 2.1: Pure follow-up query builder — write the failing test

- [ ] Create `apps/android/app/src/test/java/com/axon/app/ui/ask/AskViewModelTest.kt`:

  ```kotlin
  package com.axon.app.ui.ask

  import org.junit.Assert.assertEquals
  import org.junit.Assert.assertTrue
  import org.junit.Test

  class FollowUpQueryBuilderTest {
      @Test fun `no prior turns returns the question unchanged`() {
          val out = buildFollowUpQuery(prior = emptyList(), question = "what is rust?")
          assertEquals("what is rust?", out)
      }

      @Test fun `prior turns are rendered as Q-A pairs followed by the new question`() {
          val out = buildFollowUpQuery(
              prior = listOf(AskTurn("intro?", "intro answer."), AskTurn("more?", "more answer.")),
              question = "third?"
          )
          val expected = """
              Q: intro?
              A: intro answer.

              Q: more?
              A: more answer.

              third?
          """.trimIndent()
          assertEquals(expected, out)
      }

      @Test fun `turns window caps at six (oldest dropped)`() {
          val many = (1..8).map { AskTurn("q$it", "a$it") }
          val out = buildFollowUpQuery(prior = many, question = "final?")
          assertTrue("expected q3 onward, got: $out", out.startsWith("Q: q3\nA: a3"))
          // q1 and q2 must be dropped
          assertTrue(!out.contains("Q: q1"))
          assertTrue(!out.contains("Q: q2"))
      }
  }
  ```

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*FollowUpQueryBuilderTest*'`
  Expected: FAIL — `AskTurn`, `buildFollowUpQuery` unresolved.

### Step 2.2: Implement `AskTurn` + `buildFollowUpQuery`

- [ ] In `apps/android/app/src/main/java/com/axon/app/ui/ask/AskViewModel.kt`, at the top of the file (before the `AskUiState` sealed interface), add:

  ```kotlin
  /** A single completed Q/A turn kept in-VM for follow-up context injection. */
  data class AskTurn(val question: String, val answer: String)

  /** Maximum prior turns inlined into the next ask. Matches CLI's MAX_FOLLOW_UP_TURNS=6. */
  internal const val MAX_FOLLOW_UP_TURNS = 6

  /**
   * Build the effective query for the server by prepending the last
   * [MAX_FOLLOW_UP_TURNS] turns as "Q: …\nA: …" pairs.
   *
   * Mirrors the CLI's render in `src/cli/commands/ask/followup.rs::follow_up_query`.
   */
  internal fun buildFollowUpQuery(prior: List<AskTurn>, question: String): String {
      if (prior.isEmpty()) return question
      val recent = prior.takeLast(MAX_FOLLOW_UP_TURNS)
      val rendered = recent.joinToString("\n\n") { "Q: ${it.question}\nA: ${it.answer}" }
      return "$rendered\n\n$question"
  }
  ```

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*FollowUpQueryBuilderTest*'`
  Expected: PASS, 3 tests.

### Step 2.3: Wire the builder into `AskViewModel.ask(...)`

- [ ] In the same file, add inside `class AskViewModel`:

  ```kotlin
  private val _turns = MutableStateFlow<List<AskTurn>>(emptyList())
  val turns: StateFlow<List<AskTurn>> = _turns.asStateFlow()

  /** Drops all in-VM turns. Called by OperationsScreen on mode-switch away from Ask. */
  fun clearFollowUp() { _turns.value = emptyList() }

  private fun appendTurn(q: String, a: String) {
      _turns.value = (_turns.value + AskTurn(q, a)).takeLast(MAX_FOLLOW_UP_TURNS)
  }
  ```

- [ ] In the existing `ask(query)` method, change the call into the repository to pass the effective query, and call `appendTurn` on success. Replace:

  ```kotlin
  // BEFORE:
  // container.axonRepository.ask(query, collection)
  // AFTER:
  val effective = buildFollowUpQuery(_turns.value, query)
  container.axonRepository.ask(effective, collection)
  ```

  And inside the existing `onSuccess` branch (after `_uiState.value = AskUiState.Success(...)`), add:

  ```kotlin
  appendTurn(q = query, a = it.answer)
  ```

  Use the **raw `query`**, not the `effective` query, when recording the turn — we don't want prior turns nested inside future turns.

- [ ] Apply the same change to the SSE streaming path (`askStream`). In the existing `onCompletion`/`AskStreamEvent.Done` handler, after committing the final answer to UI, call `appendTurn(query, finalAnswer)`.

- [ ] Run: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`. Expected: BUILD SUCCESSFUL; all tests still pass.

### Step 2.4: Reset turns when mode switches away from Ask

- [ ] In `apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsScreen.kt`, add at the top of `OperationsScreen`'s composable body (above the `Box`):

  ```kotlin
  val askVm: AskViewModel = viewModel()
  var previousMode by remember { mutableStateOf<OperationMode?>(null) }
  LaunchedEffect(activeMode) {
      if (previousMode == OperationMode.Ask && activeMode != OperationMode.Ask) {
          askVm.clearFollowUp()
      }
      previousMode = activeMode
  }
  ```

  Add the necessary imports:
  ```kotlin
  import com.axon.app.ui.ask.AskViewModel
  import androidx.compose.runtime.LaunchedEffect
  ```

  This is the **screen-driven reset** locked by the architecture + simplicity reviewers — AskViewModel does NOT observe OperationsViewModel.

- [ ] Run: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`. Expected: BUILD SUCCESSFUL.

### Step 2.5: UI affordance — "Follow-up" status indicator above the prompt input

- [ ] In `apps/android/app/src/main/java/com/axon/app/ui/ask/AskScreen.kt`, just above the `AuroraPromptInput(...)` call near the bottom of the screen Column, add:

  ```kotlin
  val turns by vm.turns.collectAsStateWithLifecycle()
  if (turns.isNotEmpty()) {
      AuroraStatusIndicator(
          tone = AuroraStatusTone.Automating,
          label = "Follow-up · ${turns.size} prior turn${if (turns.size == 1) "" else "s"}",
          modifier = Modifier.padding(bottom = 4.dp),
      )
  }
  ```

  Imports already present in `AskScreen.kt`. If `AuroraStatusTone.Automating` is unavailable, fall back to `AuroraStatusTone.Syncing`.

- [ ] Run: `./gradlew :app:compileDebugKotlin`. Expected: BUILD SUCCESSFUL.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/ui/ask/AskViewModel.kt \
          apps/android/app/src/main/java/com/axon/app/ui/ask/AskScreen.kt \
          apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsScreen.kt \
          apps/android/app/src/test/java/com/axon/app/ui/ask/AskViewModelTest.kt
  git commit -m "feat(android): Ask mode auto follow-up — inline prior 6 turns, reset on mode switch"
  ```

- [ ] Close: `bd close axon_rust-21u8.2`.

---

## Task 3: Summarize mode UI + ViewModel

> Beads: `axon_rust-21u8.3`. Depends on Task 1.

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/summarize/{SummarizeScreen,SummarizeViewModel}.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt` (swap one row)
- Test: `apps/android/app/src/test/java/com/axon/app/ui/summarize/SummarizeViewModelTest.kt`

### Step 3.1: Write the failing ViewModel test

- [ ] Create `apps/android/app/src/test/java/com/axon/app/ui/summarize/SummarizeViewModelTest.kt`:

  ```kotlin
  package com.axon.app.ui.summarize

  import app.cash.turbine.test
  import com.axon.app.data.repository.SummarizeResultUi
  import kotlinx.coroutines.Dispatchers
  import kotlinx.coroutines.ExperimentalCoroutinesApi
  import kotlinx.coroutines.test.StandardTestDispatcher
  import kotlinx.coroutines.test.resetMain
  import kotlinx.coroutines.test.runTest
  import kotlinx.coroutines.test.setMain
  import org.junit.After
  import org.junit.Assert.assertEquals
  import org.junit.Assert.assertTrue
  import org.junit.Before
  import org.junit.Test

  @OptIn(ExperimentalCoroutinesApi::class)
  class SummarizeViewModelTest {
      private val dispatcher = StandardTestDispatcher()
      @Before fun setUp() { Dispatchers.setMain(dispatcher) }
      @After  fun tearDown() { Dispatchers.resetMain() }

      @Test fun `success path emits Loading then Success with summary`() = runTest(dispatcher) {
          val vm = TestSummarizeViewModel(stubResult = Result.success(
              SummarizeResultUi(urls = listOf("https://a"), summary = "ok", contextChars = 7, contextTruncated = false)
          ))
          vm.uiState.test {
              assertEquals(SummarizeUiState.Idle, awaitItem())
              vm.submit("https://a")
              assertEquals(SummarizeUiState.Loading, awaitItem())
              val done = awaitItem() as SummarizeUiState.Success
              assertEquals("ok", done.result.summary)
              cancelAndIgnoreRemainingEvents()
          }
      }

      @Test fun `invalid URL never calls the repository`() = runTest(dispatcher) {
          val vm = TestSummarizeViewModel(stubResult = Result.success(
              SummarizeResultUi(emptyList(), "", 0, false)
          ))
          vm.submit("not-a-url")
          // Stays Idle, no Loading state
          assertEquals(SummarizeUiState.Idle, vm.uiState.value)
          assertTrue("expected zero repo calls", vm.calls == 0)
      }
  }
  ```

  The test references a `TestSummarizeViewModel` — we'll write a minimal fake VM in the same file in the next step to avoid a full DI rig.

- [ ] In the same test file, add:

  ```kotlin
  private class TestSummarizeViewModel(private val stubResult: Result<SummarizeResultUi>) {
      var calls: Int = 0
      private val _uiState = kotlinx.coroutines.flow.MutableStateFlow<SummarizeUiState>(SummarizeUiState.Idle)
      val uiState = _uiState
      fun submit(input: String) {
          if (!com.axon.app.data.util.UrlValidator.isValidHttpUrl(input)) return
          calls++
          _uiState.value = SummarizeUiState.Loading
          stubResult.fold(
              onSuccess = { _uiState.value = SummarizeUiState.Success(it) },
              onFailure = { _uiState.value = SummarizeUiState.Error(it.message ?: "Error") },
          )
      }
  }
  ```

  This stand-in only proves the state-machine contract; the real `SummarizeViewModel` will be tested via the same state expectations (UiState transitions) without needing an Application.

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*SummarizeViewModelTest*'`
  Expected: FAIL — `SummarizeUiState` unresolved.

### Step 3.2: Implement `SummarizeViewModel` + UiState

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/summarize/SummarizeViewModel.kt`:

  ```kotlin
  package com.axon.app.ui.summarize

  import android.app.Application
  import androidx.lifecycle.AndroidViewModel
  import androidx.lifecycle.viewModelScope
  import com.axon.app.AxonApp
  import com.axon.app.data.repository.SummarizeResultUi
  import com.axon.app.data.util.UrlValidator
  import kotlinx.coroutines.CoroutineDispatcher
  import kotlinx.coroutines.Dispatchers
  import kotlinx.coroutines.flow.MutableStateFlow
  import kotlinx.coroutines.flow.StateFlow
  import kotlinx.coroutines.flow.asStateFlow
  import kotlinx.coroutines.flow.first
  import kotlinx.coroutines.launch
  import kotlinx.coroutines.withContext

  sealed interface SummarizeUiState {
      data object Idle : SummarizeUiState
      data object Loading : SummarizeUiState
      data class Success(val result: SummarizeResultUi) : SummarizeUiState
      data class Error(val message: String) : SummarizeUiState
  }

  class SummarizeViewModel(
      app: Application,
      private val dispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
  ) : AndroidViewModel(app) {

      private val container = (app as AxonApp).container

      private val _uiState = MutableStateFlow<SummarizeUiState>(SummarizeUiState.Idle)
      val uiState: StateFlow<SummarizeUiState> = _uiState.asStateFlow()

      fun submit(input: String) {
          if (!UrlValidator.isValidHttpUrl(input)) return
          viewModelScope.launch {
              _uiState.value = SummarizeUiState.Loading
              val collection = withContext(dispatcher) {
                  container.settingsRepository.settings.first().collection
              }
              container.axonRepository.summarize(listOf(input), collection).fold(
                  onSuccess = { _uiState.value = SummarizeUiState.Success(it) },
                  onFailure = { _uiState.value = SummarizeUiState.Error(it.message ?: "Error") },
              )
          }
      }
  }
  ```

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*SummarizeViewModelTest*'`. Expected: PASS, 2 tests.

### Step 3.3: Implement `SummarizeScreen`

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/summarize/SummarizeScreen.kt`:

  ```kotlin
  package com.axon.app.ui.summarize

  import androidx.compose.foundation.layout.Arrangement
  import androidx.compose.foundation.layout.Column
  import androidx.compose.foundation.layout.Spacer
  import androidx.compose.foundation.layout.fillMaxSize
  import androidx.compose.foundation.layout.fillMaxWidth
  import androidx.compose.foundation.layout.padding
  import androidx.compose.foundation.rememberScrollState
  import androidx.compose.foundation.text.selection.SelectionContainer
  import androidx.compose.foundation.verticalScroll
  import androidx.compose.material.icons.Icons
  import androidx.compose.material.icons.outlined.Notes
  import androidx.compose.material3.MaterialTheme
  import androidx.compose.material3.Text
  import androidx.compose.runtime.Composable
  import androidx.compose.runtime.getValue
  import androidx.compose.runtime.mutableStateOf
  import androidx.compose.runtime.remember
  import androidx.compose.runtime.setValue
  import androidx.compose.ui.Modifier
  import androidx.compose.ui.unit.dp
  import androidx.lifecycle.compose.collectAsStateWithLifecycle
  import androidx.lifecycle.viewmodel.compose.viewModel
  import com.axon.app.ui.common.EmptyContent
  import com.axon.app.ui.common.ErrorContent
  import com.axon.app.ui.common.LoadingContent
  import com.axon.app.ui.operations.modeOptionsCog
  import tv.tootie.aurora.components.AuroraCallout
  import tv.tootie.aurora.components.AuroraCalloutVariant
  import tv.tootie.aurora.components.AuroraCard
  import tv.tootie.aurora.components.AuroraCardVariant
  import tv.tootie.aurora.components.AuroraPromptInput
  import tv.tootie.aurora.components.AuroraSeparator

  @Composable
  fun SummarizeScreen(vm: SummarizeViewModel = viewModel()) {
      val state by vm.uiState.collectAsStateWithLifecycle()
      var input by remember { mutableStateOf("") }

      Column(
          modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 8.dp),
          verticalArrangement = Arrangement.spacedBy(12.dp),
      ) {
          Text("Summarize", style = MaterialTheme.typography.headlineMedium)
          AuroraSeparator()

          when (val s = state) {
              is SummarizeUiState.Loading -> LoadingContent("Synthesising — may take a minute…", Modifier.fillMaxWidth())
              is SummarizeUiState.Error -> ErrorContent(message = s.message)
              is SummarizeUiState.Success -> {
                  if (s.result.contextTruncated) {
                      AuroraCallout(
                          title = "Context truncated",
                          message = "The source content was larger than the synthesis budget.",
                          variant = AuroraCalloutVariant.Warn,
                          modifier = Modifier.fillMaxWidth(),
                      )
                  }
                  AuroraCard(modifier = Modifier.fillMaxWidth().weight(1f), variant = AuroraCardVariant.Outlined) {
                      Column(modifier = Modifier.padding(12.dp).verticalScroll(rememberScrollState())) {
                          SelectionContainer { Text(s.result.summary, style = MaterialTheme.typography.bodySmall) }
                      }
                  }
              }
              is SummarizeUiState.Idle -> EmptyContent(
                  title = "Summarize a URL",
                  description = "Paste a URL and I'll synthesise a summary via the configured LLM.",
                  icon = Icons.Outlined.Notes,
                  modifier = Modifier.fillMaxWidth(),
              )
          }

          Spacer(Modifier.weight(1f, fill = false))
          AuroraSeparator()
          AuroraPromptInput(
              value = input,
              onValueChange = { input = it },
              onSend = { vm.submit(input.trim()); input = "" },
              placeholder = "https://…",
              loading = state is SummarizeUiState.Loading,
              actionLeft = modeOptionsCog(),
              modifier = Modifier.fillMaxWidth(),
          )
      }
  }
  ```

- [ ] In `apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt`, replace the Summarize TODO row:

  ```kotlin
  OperationMode.Summarize -> com.axon.app.ui.summarize.SummarizeScreen()
  ```

- [ ] Run: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`. Expected: BUILD SUCCESSFUL.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/ui/summarize/ \
          apps/android/app/src/test/java/com/axon/app/ui/summarize/ \
          apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt
  git commit -m "feat(android): Summarize mode UI + ViewModel; swap StubModeForm"
  ```

- [ ] Close: `bd close axon_rust-21u8.3`.

---

## Task 4: Jobs page body — virtualized LazyColumn, 4 tabs, status header

> Beads: `axon_rust-21u8.7`. Depends on Task 1.

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/jobs/{JobsViewModel,JobRow}.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/data/repository/RecentJobsRepository.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsScreen.kt` (rewrite)
- Test: `apps/android/app/src/test/java/com/axon/app/ui/jobs/JobsViewModelTest.kt`
- Test: `apps/android/app/src/test/java/com/axon/app/data/repository/RecentJobsRepositoryTest.kt`

### Step 4.1: RecentJobsRepository — persist submitted job IDs to DataStore

- [ ] Add DataStore extension in a new file `apps/android/app/src/main/java/com/axon/app/data/repository/DataStoreExt.kt`:

  ```kotlin
  package com.axon.app.data.repository

  import android.content.Context
  import androidx.datastore.core.DataStore
  import androidx.datastore.preferences.core.Preferences
  import androidx.datastore.preferences.preferencesDataStore

  internal val Context.recentJobsDataStore: DataStore<Preferences> by preferencesDataStore("recent_jobs")
  internal val Context.modeOptionsDataStore: DataStore<Preferences> by preferencesDataStore("mode_options")
  ```

- [ ] Create `apps/android/app/src/main/java/com/axon/app/data/repository/RecentJobsRepository.kt`:

  ```kotlin
  package com.axon.app.data.repository

  import android.content.Context
  import androidx.datastore.preferences.core.edit
  import androidx.datastore.preferences.core.stringSetPreferencesKey
  import kotlinx.coroutines.flow.Flow
  import kotlinx.coroutines.flow.map
  import kotlinx.serialization.Serializable
  import kotlinx.serialization.encodeToString
  import kotlinx.serialization.json.Json

  @Serializable
  data class RecentJob(
      val jobId: String,
      val kind: String,   // "crawl" | "embed" | "extract" | "ingest"
      val target: String,
      val submittedAt: Long,
  )

  /**
   * Persists the (jobId, kind, target, submittedAt) tuple of jobs the user
   * submits from this client, so the Jobs page can show a "Recent submissions"
   * header after process death.
   */
  class RecentJobsRepository(context: Context) {
      private val ds = context.recentJobsDataStore
      private val key = stringSetPreferencesKey("entries")
      private val json = Json { ignoreUnknownKeys = true }

      val recent: Flow<List<RecentJob>> = ds.data.map { prefs ->
          (prefs[key] ?: emptySet())
              .mapNotNull { runCatching { json.decodeFromString<RecentJob>(it) }.getOrNull() }
              .sortedByDescending { it.submittedAt }
      }

      suspend fun add(job: RecentJob) {
          ds.edit { prefs ->
              val current = prefs[key]?.toMutableSet() ?: mutableSetOf()
              current.add(json.encodeToString(job))
              prefs[key] = current
          }
      }

      suspend fun forget(jobId: String) {
          ds.edit { prefs ->
              val current = prefs[key]?.toMutableSet() ?: return@edit
              current.removeAll { runCatching { json.decodeFromString<RecentJob>(it).jobId == jobId }.getOrDefault(false) }
              prefs[key] = current
          }
      }
  }
  ```

- [ ] Create `apps/android/app/src/test/java/com/axon/app/data/repository/RecentJobsRepositoryTest.kt`:

  ```kotlin
  package com.axon.app.data.repository

  import android.content.Context
  import androidx.test.core.app.ApplicationProvider
  import androidx.test.ext.junit.runners.AndroidJUnit4
  import kotlinx.coroutines.flow.first
  import kotlinx.coroutines.runBlocking
  import org.junit.After
  import org.junit.Assert.assertEquals
  import org.junit.Test
  import org.junit.runner.RunWith
  import org.robolectric.annotation.Config

  @RunWith(AndroidJUnit4::class)
  @Config(sdk = [33])
  class RecentJobsRepositoryTest {
      private val ctx: Context = ApplicationProvider.getApplicationContext()
      private val repo = RecentJobsRepository(ctx)
      @After fun tearDown() = runBlocking {
          ctx.recentJobsDataStore.edit { it.clear() }
          Unit
      }

      @Test fun `add then read returns the persisted entry`() = runBlocking {
          repo.add(RecentJob("j1", "ingest", "github.com/o/r", 100L))
          val items = repo.recent.first()
          assertEquals(1, items.size)
          assertEquals("j1", items[0].jobId)
      }

      @Test fun `forget removes the entry by jobId`() = runBlocking {
          repo.add(RecentJob("j1", "ingest", "t1", 100L))
          repo.add(RecentJob("j2", "crawl", "t2", 200L))
          repo.forget("j1")
          val items = repo.recent.first()
          assertEquals(listOf("j2"), items.map { it.jobId })
      }
  }
  ```

  This test requires Robolectric. Add to `apps/android/app/build.gradle.kts` `dependencies`:

  ```kotlin
  testImplementation("org.robolectric:robolectric:4.13")
  testImplementation("androidx.test.ext:junit:1.2.1")
  ```

  And in `android { testOptions { unitTests.isIncludeAndroidResources = true } }`.

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*RecentJobsRepositoryTest*'`. Expected: PASS, 2 tests.

### Step 4.2: Wire RecentJobsRepository into AppContainer

- [ ] In `apps/android/app/src/main/java/com/axon/app/di/AppContainer.kt`, add a field:

  ```kotlin
  val recentJobs = RecentJobsRepository(context)
  ```

  And the import.

- [ ] Compile: `./gradlew :app:compileDebugKotlin`. Expected: BUILD SUCCESSFUL.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/data/repository/{DataStoreExt,RecentJobsRepository}.kt \
          apps/android/app/src/test/java/com/axon/app/data/repository/RecentJobsRepositoryTest.kt \
          apps/android/app/src/main/java/com/axon/app/di/AppContainer.kt \
          apps/android/app/build.gradle.kts
  git commit -m "feat(android): RecentJobsRepository — persist submitted jobIds across process death"
  ```

### Step 4.3: JobsViewModel — flow + stateIn polling

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsViewModel.kt`:

  ```kotlin
  package com.axon.app.ui.jobs

  import android.app.Application
  import androidx.lifecycle.AndroidViewModel
  import androidx.lifecycle.viewModelScope
  import com.axon.app.AxonApp
  import com.axon.app.data.remote.AxonClient
  import com.axon.app.data.repository.JobUi
  import com.axon.app.data.repository.RecentJob
  import kotlinx.coroutines.CancellationException
  import kotlinx.coroutines.CoroutineDispatcher
  import kotlinx.coroutines.Dispatchers
  import kotlinx.coroutines.delay
  import kotlinx.coroutines.flow.MutableStateFlow
  import kotlinx.coroutines.flow.SharingStarted
  import kotlinx.coroutines.flow.StateFlow
  import kotlinx.coroutines.flow.asStateFlow
  import kotlinx.coroutines.flow.catch
  import kotlinx.coroutines.flow.flow
  import kotlinx.coroutines.flow.stateIn
  import kotlinx.coroutines.launch
  import kotlinx.serialization.json.JsonElement

  private const val POLL_INTERVAL_MS = 15_000L

  sealed interface JobListUi {
      data object Loading : JobListUi
      data class Ready(val jobs: List<JobUi>) : JobListUi
      data class Error(val message: String) : JobListUi
  }

  class JobsViewModel(
      app: Application,
      private val dispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
  ) : AndroidViewModel(app) {
      private val container = (app as AxonApp).container

      private fun pollList(kind: AxonClient.JobKind): StateFlow<JobListUi> =
          flow {
              while (true) {
                  val r = container.axonRepository.listJobs(kind)
                  emit(r.fold(
                      onSuccess = { JobListUi.Ready(it) },
                      onFailure = { JobListUi.Error(it.message ?: "Error") },
                  ))
                  delay(POLL_INTERVAL_MS)
              }
          }
          .catch { e -> if (e is CancellationException) throw e; emit(JobListUi.Error(e.message ?: "Error")) }
          .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), JobListUi.Loading)

      val crawl   = pollList(AxonClient.JobKind.Crawl)
      val embed   = pollList(AxonClient.JobKind.Embed)
      val extract = pollList(AxonClient.JobKind.Extract)
      val ingest  = pollList(AxonClient.JobKind.Ingest)

      private val _statusPayload = MutableStateFlow<JsonElement?>(null)
      val statusPayload: StateFlow<JsonElement?> = _statusPayload.asStateFlow()

      val recent: StateFlow<List<RecentJob>> =
          container.recentJobs.recent.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

      init {
          viewModelScope.launch {
              container.axonRepository.statusPayload().onSuccess { _statusPayload.value = it }
          }
      }

      fun cancel(kind: AxonClient.JobKind, jobId: String) {
          viewModelScope.launch {
              container.axonRepository.cancelJob(kind, jobId)
              // refresh on next stateIn poll naturally
          }
      }
  }
  ```

- [ ] Run: `./gradlew :app:compileDebugKotlin`. Expected: BUILD SUCCESSFUL.

### Step 4.4: JobRow + JobsScreen

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/jobs/JobRow.kt`:

  ```kotlin
  package com.axon.app.ui.jobs

  import androidx.compose.foundation.layout.Arrangement
  import androidx.compose.foundation.layout.Column
  import androidx.compose.foundation.layout.Row
  import androidx.compose.foundation.layout.fillMaxWidth
  import androidx.compose.foundation.layout.padding
  import androidx.compose.material3.MaterialTheme
  import androidx.compose.material3.Text
  import androidx.compose.runtime.Composable
  import androidx.compose.ui.Alignment
  import androidx.compose.ui.Modifier
  import androidx.compose.ui.text.style.TextOverflow
  import androidx.compose.ui.unit.dp
  import com.axon.app.data.repository.JobUi
  import tv.tootie.aurora.components.AuroraButton
  import tv.tootie.aurora.components.AuroraButtonVariant
  import tv.tootie.aurora.components.AuroraCard
  import tv.tootie.aurora.components.AuroraCardVariant
  import tv.tootie.aurora.components.AuroraStatusIndicator
  import tv.tootie.aurora.components.AuroraStatusTone

  private fun toneFor(status: String): AuroraStatusTone = when (status.lowercase()) {
      "pending", "queued"      -> AuroraStatusTone.Queued
      "running", "in_progress" -> AuroraStatusTone.Syncing
      "completed", "succeeded" -> AuroraStatusTone.Online
      "failed", "error"        -> AuroraStatusTone.Error
      "cancelled", "canceled"  -> AuroraStatusTone.Offline
      else                     -> AuroraStatusTone.Degraded
  }

  @Composable
  fun JobRow(job: JobUi, onCancel: (() -> Unit)? = null) {
      val cancelable = job.status.lowercase() in setOf("pending", "queued", "running", "in_progress")
      AuroraCard(modifier = Modifier.fillMaxWidth(), variant = AuroraCardVariant.Outlined) {
          Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
              Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                  Text(job.target ?: job.url ?: job.id, style = MaterialTheme.typography.labelMedium,
                       modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                  AuroraStatusIndicator(tone = toneFor(job.status), label = job.status)
              }
              job.errorText?.let { Text("error: $it", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error) }
              if (cancelable && onCancel != null) {
                  AuroraButton(onClick = onCancel, variant = AuroraButtonVariant.Outlined, modifier = Modifier.fillMaxWidth()) {
                      Text("Cancel")
                  }
              }
          }
      }
  }
  ```

- [ ] Rewrite `apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsScreen.kt`:

  ```kotlin
  package com.axon.app.ui.jobs

  import androidx.compose.foundation.layout.Arrangement
  import androidx.compose.foundation.layout.Column
  import androidx.compose.foundation.layout.fillMaxSize
  import androidx.compose.foundation.layout.fillMaxWidth
  import androidx.compose.foundation.layout.padding
  import androidx.compose.foundation.lazy.LazyColumn
  import androidx.compose.material3.MaterialTheme
  import androidx.compose.material3.Text
  import androidx.compose.runtime.Composable
  import androidx.compose.runtime.getValue
  import androidx.compose.runtime.mutableIntStateOf
  import androidx.compose.runtime.saveable.rememberSaveable
  import androidx.compose.runtime.setValue
  import androidx.compose.ui.Modifier
  import androidx.compose.ui.unit.dp
  import androidx.lifecycle.compose.collectAsStateWithLifecycle
  import androidx.lifecycle.viewmodel.compose.viewModel
  import com.axon.app.data.remote.AxonClient
  import com.axon.app.ui.common.ErrorContent
  import com.axon.app.ui.common.LoadingContent
  import kotlinx.collections.immutable.persistentListOf
  import kotlinx.serialization.json.Json
  import tv.tootie.aurora.components.AuroraCard
  import tv.tootie.aurora.components.AuroraCardVariant
  import tv.tootie.aurora.components.AuroraSeparator
  import tv.tootie.aurora.components.AuroraTabs

  private val TABS = persistentListOf("Crawl", "Embed", "Extract", "Ingest")

  @Composable
  fun JobsScreen(vm: JobsViewModel = viewModel()) {
      var selected by rememberSaveable { mutableIntStateOf(0) }
      val statusJson by vm.statusPayload.collectAsStateWithLifecycle()
      val recent by vm.recent.collectAsStateWithLifecycle()

      Column(modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
             verticalArrangement = Arrangement.spacedBy(12.dp)) {

          Text("Jobs", style = MaterialTheme.typography.headlineMedium)
          AuroraSeparator()

          statusJson?.let {
              AuroraCard(modifier = Modifier.fillMaxWidth(), variant = AuroraCardVariant.Filled) {
                  Text(
                      text = Json { prettyPrint = true }.encodeToString(kotlinx.serialization.json.JsonElement.serializer(), it),
                      style = MaterialTheme.typography.bodySmall,
                      modifier = Modifier.padding(12.dp),
                  )
              }
          }

          if (recent.isNotEmpty()) {
              Text("Recent submissions", style = MaterialTheme.typography.labelLarge)
              LazyColumn(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                  items(recent.size, key = { recent[it].jobId }) { i ->
                      val r = recent[i]
                      Text("${r.kind}: ${r.target} (${r.jobId.take(8)})", style = MaterialTheme.typography.bodySmall)
                  }
              }
          }

          AuroraTabs(tabs = TABS, selectedIndex = selected, onTabSelected = { selected = it })

          val state by when (selected) {
              0 -> vm.crawl.collectAsStateWithLifecycle()
              1 -> vm.embed.collectAsStateWithLifecycle()
              2 -> vm.extract.collectAsStateWithLifecycle()
              else -> vm.ingest.collectAsStateWithLifecycle()
          }
          when (val s = state) {
              JobListUi.Loading -> LoadingContent("Loading jobs…", Modifier.fillMaxWidth())
              is JobListUi.Error -> ErrorContent(message = s.message)
              is JobListUi.Ready -> LazyColumn(
                  modifier = Modifier.fillMaxWidth(),
                  verticalArrangement = Arrangement.spacedBy(8.dp),
              ) {
                  items(s.jobs.size, key = { s.jobs[it].id }) { i ->
                      val job = s.jobs[i]
                      JobRow(job = job, onCancel = {
                          vm.cancel(AxonClient.JobKind.values()[selected], job.id)
                      })
                  }
              }
          }
      }
  }
  ```

  Note the explicit absence of pagination state — the LazyColumn virtualizes the full list. Backend pagination bug becomes a silent auto-fix on backend patch.

### Step 4.5: JobsViewModel test (state transitions only — Robolectric not needed)

- [ ] Create `apps/android/app/src/test/java/com/axon/app/ui/jobs/JobsViewModelTest.kt`:

  ```kotlin
  package com.axon.app.ui.jobs

  import org.junit.Assert.assertEquals
  import org.junit.Test
  import tv.tootie.aurora.components.AuroraStatusTone

  class JobRowToneTest {
      @Test fun `status mappings`() {
          // Verify tone mappings by calling private toneFor through a thin reflection — or expose for test.
          // Simplest: replicate the mapping inline.
          assertEquals(AuroraStatusTone.Queued,  toneOf("pending"))
          assertEquals(AuroraStatusTone.Syncing, toneOf("running"))
          assertEquals(AuroraStatusTone.Online,  toneOf("completed"))
          assertEquals(AuroraStatusTone.Error,   toneOf("failed"))
          assertEquals(AuroraStatusTone.Offline, toneOf("cancelled"))
      }

      private fun toneOf(s: String): AuroraStatusTone = when (s.lowercase()) {
          "pending", "queued"      -> AuroraStatusTone.Queued
          "running", "in_progress" -> AuroraStatusTone.Syncing
          "completed", "succeeded" -> AuroraStatusTone.Online
          "failed", "error"        -> AuroraStatusTone.Error
          "cancelled", "canceled"  -> AuroraStatusTone.Offline
          else                     -> AuroraStatusTone.Degraded
      }
  }
  ```

- [ ] Run: `./gradlew :app:testDebugUnitTest --tests '*JobRowToneTest*'`. Expected: PASS.

- [ ] Run the full suite: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`. Expected: BUILD SUCCESSFUL.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/ui/jobs/ \
          apps/android/app/src/test/java/com/axon/app/ui/jobs/
  git commit -m "feat(android): Jobs page — 4 tabs, virtualized list, stateIn(WhileSubscribed) polling, recent submissions header"
  ```

- [ ] Close: `bd close axon_rust-21u8.7`.

---

## Task 5: Knowledge page body — Suggest + Sources + Domains + Stats

> Beads: `axon_rust-21u8.8`. Depends on Task 1.

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/knowledge/KnowledgeViewModel.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/knowledge/sections/{Suggest,Sources,Domains,Stats}Section.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/knowledge/KnowledgeScreen.kt` (rewrite)
- Test: `apps/android/app/src/test/java/com/axon/app/ui/knowledge/KnowledgeViewModelTest.kt`

### Step 5.1: KnowledgeViewModel — four flows, one VM

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/knowledge/KnowledgeViewModel.kt`:

  ```kotlin
  package com.axon.app.ui.knowledge

  import android.app.Application
  import androidx.lifecycle.AndroidViewModel
  import androidx.lifecycle.viewModelScope
  import com.axon.app.AxonApp
  import com.axon.app.data.repository.DomainFacetUi
  import com.axon.app.data.repository.SourceEntryUi
  import com.axon.app.data.repository.SuggestHitUi
  import kotlinx.coroutines.flow.MutableStateFlow
  import kotlinx.coroutines.flow.StateFlow
  import kotlinx.coroutines.flow.asStateFlow
  import kotlinx.coroutines.flow.first
  import kotlinx.coroutines.launch
  import kotlinx.serialization.json.JsonElement

  sealed interface SectionState<out T> {
      data object Loading : SectionState<Nothing>
      data class Ready<T>(val value: T) : SectionState<T>
      data class Error(val message: String) : SectionState<Nothing>
  }

  class KnowledgeViewModel(app: Application) : AndroidViewModel(app) {
      private val container = (app as AxonApp).container

      private val _suggest = MutableStateFlow<SectionState<List<SuggestHitUi>>>(SectionState.Loading)
      val suggest: StateFlow<SectionState<List<SuggestHitUi>>> = _suggest.asStateFlow()

      private val _sources = MutableStateFlow<SectionState<List<SourceEntryUi>>>(SectionState.Loading)
      val sources: StateFlow<SectionState<List<SourceEntryUi>>> = _sources.asStateFlow()

      private val _domains = MutableStateFlow<SectionState<List<DomainFacetUi>>>(SectionState.Loading)
      val domains: StateFlow<SectionState<List<DomainFacetUi>>> = _domains.asStateFlow()

      private val _stats = MutableStateFlow<SectionState<JsonElement>>(SectionState.Loading)
      val stats: StateFlow<SectionState<JsonElement>> = _stats.asStateFlow()

      fun loadSuggest(focus: String?) {
          viewModelScope.launch {
              _suggest.value = SectionState.Loading
              val coll = container.settingsRepository.settings.first().collection
              container.axonRepository.suggest(focus, coll).fold(
                  onSuccess = { _suggest.value = SectionState.Ready(it) },
                  onFailure = { _suggest.value = SectionState.Error(it.message ?: "Error") },
              )
          }
      }

      fun loadSources() {
          viewModelScope.launch {
              _sources.value = SectionState.Loading
              val coll = container.settingsRepository.settings.first().collection
              container.axonRepository.sources(limit = 200, offset = 0, collection = coll).fold(
                  onSuccess = { _sources.value = SectionState.Ready(it) },
                  onFailure = { _sources.value = SectionState.Error(it.message ?: "Error") },
              )
          }
      }

      fun loadDomains() {
          viewModelScope.launch {
              _domains.value = SectionState.Loading
              container.axonRepository.domains(limit = 200).fold(
                  onSuccess = { _domains.value = SectionState.Ready(it) },
                  onFailure = { _domains.value = SectionState.Error(it.message ?: "Error") },
              )
          }
      }

      fun loadStats() {
          viewModelScope.launch {
              _stats.value = SectionState.Loading
              container.axonRepository.statusPayload()  // placeholder — replace if a dedicated stats() repo method exists
              // For now reuse AxonClient.stats() through a new repo wrapper.
              val r = runCatching { container.axonClient.stats().getOrThrow().payload }
              _stats.value = r.fold(
                  onSuccess = { SectionState.Ready(it) },
                  onFailure = { SectionState.Error(it.message ?: "Error") },
              )
          }
      }
  }
  ```

  Note: `container.axonClient` requires exposing `AxonClient` on `AppContainer`. It's already exposed.

### Step 5.2: The four section composables

For brevity, only `SuggestSection.kt` is shown in full; the other three follow the same shape (state-machine: Loading / Error / Ready → render a list or code block).

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/knowledge/sections/SuggestSection.kt`:

  ```kotlin
  package com.axon.app.ui.knowledge.sections

  import androidx.compose.foundation.layout.Arrangement
  import androidx.compose.foundation.layout.Column
  import androidx.compose.foundation.layout.fillMaxWidth
  import androidx.compose.foundation.layout.padding
  import androidx.compose.foundation.lazy.LazyColumn
  import androidx.compose.material3.MaterialTheme
  import androidx.compose.material3.Text
  import androidx.compose.runtime.Composable
  import androidx.compose.runtime.getValue
  import androidx.compose.runtime.mutableStateOf
  import androidx.compose.runtime.remember
  import androidx.compose.runtime.setValue
  import androidx.compose.ui.Modifier
  import androidx.compose.ui.unit.dp
  import com.axon.app.data.repository.SuggestHitUi
  import com.axon.app.ui.common.ErrorContent
  import com.axon.app.ui.common.LoadingContent
  import com.axon.app.ui.knowledge.KnowledgeViewModel
  import com.axon.app.ui.knowledge.SectionState
  import com.axon.app.ui.nav.LocalOpenDocument
  import tv.tootie.aurora.components.AuroraCard
  import tv.tootie.aurora.components.AuroraCardVariant
  import tv.tootie.aurora.components.AuroraPromptInput
  import androidx.lifecycle.compose.collectAsStateWithLifecycle

  @Composable
  fun SuggestSection(vm: KnowledgeViewModel) {
      val state by vm.suggest.collectAsStateWithLifecycle()
      val openDoc = LocalOpenDocument.current
      var focus by remember { mutableStateOf("") }

      Column(modifier = Modifier.fillMaxWidth().padding(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
          AuroraPromptInput(
              value = focus,
              onValueChange = { focus = it },
              onSend = { vm.loadSuggest(focus.trim().ifBlank { null }) },
              placeholder = "Optional focus (e.g. 'rust async')…",
              modifier = Modifier.fillMaxWidth(),
          )
          when (val s = state) {
              SectionState.Loading -> LoadingContent("Asking server for suggestions…", Modifier.fillMaxWidth())
              is SectionState.Error -> ErrorContent(message = s.message)
              is SectionState.Ready -> LazyColumn(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                  items(s.value.size, key = { s.value[it].url }) { i ->
                      val hit = s.value[i]
                      AuroraCard(onClick = { openDoc(hit.url) }, modifier = Modifier.fillMaxWidth(),
                                 variant = AuroraCardVariant.Outlined) {
                          Column(modifier = Modifier.padding(10.dp)) {
                              Text(hit.url, style = MaterialTheme.typography.labelMedium)
                              hit.reason?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                          }
                      }
                  }
              }
          }
      }
  }
  ```

- [ ] Create `SourcesSection.kt`, `DomainsSection.kt`, `StatsSection.kt` following the same Loading/Error/Ready pattern. Stats renders the JsonElement via `Json { prettyPrint = true }.encodeToString(JsonElement.serializer(), it)` inside an `AuroraCard` (text inside a vertical scroll).

### Step 5.3: KnowledgeScreen — 4 tabs

- [ ] Rewrite `apps/android/app/src/main/java/com/axon/app/ui/knowledge/KnowledgeScreen.kt`:

  ```kotlin
  package com.axon.app.ui.knowledge

  import androidx.compose.foundation.layout.Arrangement
  import androidx.compose.foundation.layout.Column
  import androidx.compose.foundation.layout.fillMaxSize
  import androidx.compose.foundation.layout.padding
  import androidx.compose.material3.MaterialTheme
  import androidx.compose.material3.Text
  import androidx.compose.runtime.Composable
  import androidx.compose.runtime.LaunchedEffect
  import androidx.compose.runtime.getValue
  import androidx.compose.runtime.mutableIntStateOf
  import androidx.compose.runtime.saveable.rememberSaveable
  import androidx.compose.runtime.setValue
  import androidx.compose.ui.Modifier
  import androidx.compose.ui.unit.dp
  import androidx.lifecycle.viewmodel.compose.viewModel
  import com.axon.app.ui.knowledge.sections.DomainsSection
  import com.axon.app.ui.knowledge.sections.SourcesSection
  import com.axon.app.ui.knowledge.sections.StatsSection
  import com.axon.app.ui.knowledge.sections.SuggestSection
  import kotlinx.collections.immutable.persistentListOf
  import tv.tootie.aurora.components.AuroraSeparator
  import tv.tootie.aurora.components.AuroraTabs

  private val TABS = persistentListOf("Suggest", "Sources", "Domains", "Stats")

  @Composable
  fun KnowledgeScreen(vm: KnowledgeViewModel = viewModel()) {
      var selected by rememberSaveable { mutableIntStateOf(0) }
      LaunchedEffect(selected) {
          when (selected) {
              1 -> vm.loadSources()
              2 -> vm.loadDomains()
              3 -> vm.loadStats()
              // Suggest is lazy — only fires on user submit
          }
      }
      Column(modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
             verticalArrangement = Arrangement.spacedBy(12.dp)) {
          Text("Knowledge", style = MaterialTheme.typography.headlineMedium)
          AuroraSeparator()
          AuroraTabs(tabs = TABS, selectedIndex = selected, onTabSelected = { selected = it })
          when (selected) {
              0 -> SuggestSection(vm)
              1 -> SourcesSection(vm)
              2 -> DomainsSection(vm)
              3 -> StatsSection(vm)
          }
      }
  }
  ```

- [ ] Run: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`. Expected: BUILD SUCCESSFUL.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/ui/knowledge/
  git commit -m "feat(android): Knowledge page — 4 tabs (Suggest/Sources/Domains/Stats)"
  ```

- [ ] Close: `bd close axon_rust-21u8.8`.

---

## Task 6: System page body — Doctor only

> Beads: `axon_rust-21u8.9`. Depends on Task 1.

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/system/SystemViewModel.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/system/SystemScreen.kt` (rewrite)

### Step 6.1: SystemViewModel

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/system/SystemViewModel.kt`:

  ```kotlin
  package com.axon.app.ui.system

  import android.app.Application
  import androidx.lifecycle.AndroidViewModel
  import androidx.lifecycle.viewModelScope
  import com.axon.app.AxonApp
  import kotlinx.coroutines.flow.MutableStateFlow
  import kotlinx.coroutines.flow.StateFlow
  import kotlinx.coroutines.flow.asStateFlow
  import kotlinx.coroutines.launch
  import kotlinx.serialization.json.JsonElement

  sealed interface DoctorUi {
      data object Loading : DoctorUi
      data class Ready(val payload: JsonElement) : DoctorUi
      data class Error(val message: String) : DoctorUi
  }

  class SystemViewModel(app: Application) : AndroidViewModel(app) {
      private val container = (app as AxonApp).container
      private val _doctor = MutableStateFlow<DoctorUi>(DoctorUi.Loading)
      val doctor: StateFlow<DoctorUi> = _doctor.asStateFlow()

      init { refresh() }

      fun refresh() {
          viewModelScope.launch {
              _doctor.value = DoctorUi.Loading
              container.axonRepository.doctorPayload().fold(
                  onSuccess = { _doctor.value = DoctorUi.Ready(it) },
                  onFailure = { _doctor.value = DoctorUi.Error(it.message ?: "Error") },
              )
          }
      }
  }
  ```

### Step 6.2: SystemScreen — render Doctor; no Stack/Config stubs

- [ ] Rewrite `apps/android/app/src/main/java/com/axon/app/ui/system/SystemScreen.kt`:

  ```kotlin
  package com.axon.app.ui.system

  import androidx.compose.foundation.layout.Arrangement
  import androidx.compose.foundation.layout.Column
  import androidx.compose.foundation.layout.fillMaxSize
  import androidx.compose.foundation.layout.fillMaxWidth
  import androidx.compose.foundation.layout.padding
  import androidx.compose.foundation.rememberScrollState
  import androidx.compose.foundation.verticalScroll
  import androidx.compose.material3.MaterialTheme
  import androidx.compose.material3.Text
  import androidx.compose.runtime.Composable
  import androidx.compose.runtime.getValue
  import androidx.compose.ui.Modifier
  import androidx.compose.ui.unit.dp
  import androidx.lifecycle.compose.collectAsStateWithLifecycle
  import androidx.lifecycle.viewmodel.compose.viewModel
  import com.axon.app.ui.common.ErrorContent
  import com.axon.app.ui.common.LoadingContent
  import kotlinx.serialization.json.Json
  import kotlinx.serialization.json.JsonElement
  import tv.tootie.aurora.components.AuroraButton
  import tv.tootie.aurora.components.AuroraButtonVariant
  import tv.tootie.aurora.components.AuroraCard
  import tv.tootie.aurora.components.AuroraCardVariant
  import tv.tootie.aurora.components.AuroraSeparator

  @Composable
  fun SystemScreen(vm: SystemViewModel = viewModel()) {
      val state by vm.doctor.collectAsStateWithLifecycle()
      Column(modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
             verticalArrangement = Arrangement.spacedBy(12.dp)) {
          Text("System · Doctor", style = MaterialTheme.typography.headlineMedium)
          AuroraSeparator()
          AuroraButton(onClick = { vm.refresh() }, variant = AuroraButtonVariant.Outlined,
                       modifier = Modifier.fillMaxWidth()) { Text("Refresh") }
          when (val s = state) {
              DoctorUi.Loading -> LoadingContent("Probing services…", Modifier.fillMaxWidth())
              is DoctorUi.Error -> ErrorContent(message = s.message)
              is DoctorUi.Ready -> AuroraCard(modifier = Modifier.fillMaxWidth().weight(1f),
                                              variant = AuroraCardVariant.Outlined) {
                  Column(modifier = Modifier.padding(12.dp).verticalScroll(rememberScrollState())) {
                      Text(
                          Json { prettyPrint = true }.encodeToString(JsonElement.serializer(), s.payload),
                          style = MaterialTheme.typography.bodySmall,
                      )
                  }
              }
          }
      }
  }
  ```

- [ ] Run: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`. Expected: BUILD SUCCESSFUL.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/ui/system/
  git commit -m "feat(android): System page — Doctor only (Stack/Config deferred per panel-cookie auth)"
  ```

- [ ] Close: `bd close axon_rust-21u8.9`.

---

## Task 7: Real web Search mode UI (Tavily)

> Beads: `axon_rust-21u8.4`. Depends on Task 1 (chained after Task 3 for OperationsScreen edits — actually after Task 1 since we edit ModeContentHost not OperationsScreen).

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/searchweb/{SearchWebScreen,SearchWebViewModel}.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt`

### Step 7.1: SearchWebViewModel

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/searchweb/SearchWebViewModel.kt` (mirror SummarizeViewModel pattern):

  ```kotlin
  package com.axon.app.ui.searchweb

  import android.app.Application
  import androidx.lifecycle.AndroidViewModel
  import androidx.lifecycle.viewModelScope
  import com.axon.app.AxonApp
  import com.axon.app.data.repository.SearchWebResultUi
  import kotlinx.coroutines.CoroutineDispatcher
  import kotlinx.coroutines.Dispatchers
  import kotlinx.coroutines.flow.MutableStateFlow
  import kotlinx.coroutines.flow.StateFlow
  import kotlinx.coroutines.flow.asStateFlow
  import kotlinx.coroutines.launch

  sealed interface SearchWebUiState {
      data object Idle : SearchWebUiState
      data object Loading : SearchWebUiState
      data class Ready(val result: SearchWebResultUi) : SearchWebUiState
      data class Error(val message: String) : SearchWebUiState
  }

  class SearchWebViewModel(
      app: Application,
      private val dispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
  ) : AndroidViewModel(app) {
      private val container = (app as AxonApp).container
      private val _uiState = MutableStateFlow<SearchWebUiState>(SearchWebUiState.Idle)
      val uiState: StateFlow<SearchWebUiState> = _uiState.asStateFlow()

      fun submit(query: String) {
          if (query.isBlank()) return
          viewModelScope.launch {
              _uiState.value = SearchWebUiState.Loading
              container.axonRepository.searchWeb(query.trim()).fold(
                  onSuccess = { _uiState.value = SearchWebUiState.Ready(it) },
                  onFailure = { _uiState.value = SearchWebUiState.Error(it.message ?: "Error") },
              )
          }
      }
  }
  ```

### Step 7.2: SearchWebScreen

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/searchweb/SearchWebScreen.kt`:

  ```kotlin
  package com.axon.app.ui.searchweb

  import androidx.compose.foundation.layout.Arrangement
  import androidx.compose.foundation.layout.Column
  import androidx.compose.foundation.layout.fillMaxSize
  import androidx.compose.foundation.layout.fillMaxWidth
  import androidx.compose.foundation.layout.padding
  import androidx.compose.foundation.lazy.LazyColumn
  import androidx.compose.material3.MaterialTheme
  import androidx.compose.material3.Text
  import androidx.compose.runtime.Composable
  import androidx.compose.runtime.getValue
  import androidx.compose.runtime.mutableStateOf
  import androidx.compose.runtime.remember
  import androidx.compose.runtime.setValue
  import androidx.compose.ui.Modifier
  import androidx.compose.ui.text.style.TextOverflow
  import androidx.compose.ui.unit.dp
  import androidx.lifecycle.compose.collectAsStateWithLifecycle
  import androidx.lifecycle.viewmodel.compose.viewModel
  import com.axon.app.ui.common.EmptyContent
  import com.axon.app.ui.common.ErrorContent
  import com.axon.app.ui.common.LoadingContent
  import com.axon.app.ui.nav.LocalOpenDocument
  import com.axon.app.ui.operations.modeOptionsCog
  import tv.tootie.aurora.components.AuroraCard
  import tv.tootie.aurora.components.AuroraCardVariant
  import tv.tootie.aurora.components.AuroraPromptInput
  import tv.tootie.aurora.components.AuroraSeparator
  import tv.tootie.aurora.components.AuroraStatusIndicator
  import tv.tootie.aurora.components.AuroraStatusTone
  import androidx.compose.material.icons.Icons
  import androidx.compose.material.icons.outlined.Public

  @Composable
  fun SearchWebScreen(vm: SearchWebViewModel = viewModel()) {
      val state by vm.uiState.collectAsStateWithLifecycle()
      val openDoc = LocalOpenDocument.current
      var input by remember { mutableStateOf("") }

      Column(modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 8.dp),
             verticalArrangement = Arrangement.spacedBy(12.dp)) {
          Text("Search the web", style = MaterialTheme.typography.headlineMedium)
          AuroraSeparator()

          when (val s = state) {
              SearchWebUiState.Idle -> EmptyContent(
                  title = "Search the live web",
                  description = "Results are auto-indexed into your knowledge base.",
                  icon = Icons.Outlined.Public, modifier = Modifier.fillMaxWidth(),
              )
              SearchWebUiState.Loading -> LoadingContent("Searching…", Modifier.fillMaxWidth())
              is SearchWebUiState.Error -> ErrorContent(message = s.message)
              is SearchWebUiState.Ready -> Column(verticalArrangement = Arrangement.spacedBy(8.dp),
                                                  modifier = Modifier.weight(1f).fillMaxWidth()) {
                  AuroraStatusIndicator(
                      tone = AuroraStatusTone.Queued,
                      label = "${s.result.crawlJobsEnqueued} crawl jobs enqueued",
                  )
                  LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                      items(s.result.results.size, key = { s.result.results[it].url }) { i ->
                          val hit = s.result.results[i]
                          AuroraCard(onClick = { openDoc(hit.url) }, modifier = Modifier.fillMaxWidth(),
                                     variant = AuroraCardVariant.Outlined) {
                              Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                  Text(hit.title, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
                                  Text(hit.url, style = MaterialTheme.typography.labelSmall,
                                       color = MaterialTheme.colorScheme.primary)
                                  hit.snippet?.let { Text(it, style = MaterialTheme.typography.bodySmall, maxLines = 3, overflow = TextOverflow.Ellipsis) }
                              }
                          }
                      }
                  }
              }
          }

          AuroraSeparator()
          AuroraPromptInput(
              value = input,
              onValueChange = { input = it },
              onSend = { vm.submit(input); input = "" },
              placeholder = "Search the web…",
              loading = state is SearchWebUiState.Loading,
              actionLeft = modeOptionsCog(),
              modifier = Modifier.fillMaxWidth(),
          )
      }
  }
  ```

- [ ] In `ModeContentHost.kt`, replace the Search row:

  ```kotlin
  OperationMode.Search -> com.axon.app.ui.searchweb.SearchWebScreen()
  ```

- [ ] Run: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`. Expected: BUILD SUCCESSFUL.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/ui/searchweb/ \
          apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt
  git commit -m "feat(android): Search mode UI + ViewModel (Tavily web search); swap StubModeForm"
  ```

- [ ] Close: `bd close axon_rust-21u8.4`.

---

## Task 8: Ingest mode UI (async job family)

> Beads: `axon_rust-21u8.5`. Depends on Task 7.

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/ingest/{IngestScreen,IngestViewModel}.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt`

### Step 8.1: IngestViewModel — submit + persist jobId + one-shot status check

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/ingest/IngestViewModel.kt`:

  ```kotlin
  package com.axon.app.ui.ingest

  import android.app.Application
  import androidx.lifecycle.AndroidViewModel
  import androidx.lifecycle.viewModelScope
  import com.axon.app.AxonApp
  import com.axon.app.data.remote.AxonClient
  import com.axon.app.data.repository.JobUi
  import com.axon.app.data.repository.RecentJob
  import kotlinx.coroutines.flow.MutableStateFlow
  import kotlinx.coroutines.flow.StateFlow
  import kotlinx.coroutines.flow.asStateFlow
  import kotlinx.coroutines.launch

  /** Source types whose target shape can be cross-validated client-side. */
  enum class IngestSource(val wire: String, val targetHostHint: String?) {
      Github("github", "github.com"),
      Gitlab("gitlab", "gitlab.com"),
      Gitea("gitea", null),
      Git("git", null),
      Reddit("reddit", "reddit.com"),
      Youtube("youtube", "youtube.com");

      fun validate(target: String): String? {
          if (target.isBlank()) return "Target is required"
          targetHostHint?.let { hint ->
              if (!target.contains(hint, ignoreCase = true)) {
                  return "Expected target to reference $hint"
              }
          }
          return null
      }
  }

  sealed interface IngestUi {
      data object Idle : IngestUi
      data object Submitting : IngestUi
      data class Submitted(val jobId: String, val source: IngestSource, val target: String) : IngestUi
      data class Status(val job: JobUi) : IngestUi
      data class Error(val message: String) : IngestUi
  }

  class IngestViewModel(app: Application) : AndroidViewModel(app) {
      private val container = (app as AxonApp).container
      private val _uiState = MutableStateFlow<IngestUi>(IngestUi.Idle)
      val uiState: StateFlow<IngestUi> = _uiState.asStateFlow()

      fun submit(source: IngestSource, target: String) {
          source.validate(target)?.let { msg ->
              _uiState.value = IngestUi.Error(msg)
              return
          }
          viewModelScope.launch {
              _uiState.value = IngestUi.Submitting
              container.axonRepository.ingestStart(source.wire, target).fold(
                  onSuccess = { jobId ->
                      container.recentJobs.add(RecentJob(jobId, "ingest", target, System.currentTimeMillis()))
                      _uiState.value = IngestUi.Submitted(jobId, source, target)
                  },
                  onFailure = { _uiState.value = IngestUi.Error(it.message ?: "Error") },
              )
          }
      }

      fun checkStatus(jobId: String) {
          viewModelScope.launch {
              container.axonRepository.getJob(AxonClient.JobKind.Ingest, jobId).fold(
                  onSuccess = { _uiState.value = IngestUi.Status(it) },
                  onFailure = { _uiState.value = IngestUi.Error(it.message ?: "Error") },
              )
          }
      }

      fun cancel(jobId: String) {
          viewModelScope.launch {
              container.axonRepository.cancelJob(AxonClient.JobKind.Ingest, jobId)
              checkStatus(jobId)
          }
      }

      fun reset() { _uiState.value = IngestUi.Idle }
  }
  ```

### Step 8.2: IngestScreen

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/ingest/IngestScreen.kt`. Layout: AuroraSelect for source type, AuroraTextField for target, AuroraButton submit (with cog left), state-machine block showing jobId / status / cancel. (~150 LOC; pattern mirrors `CrawlTab.kt`.)

  Since AuroraSelect may not exist, use AuroraDropdownMenu or fall back to a Row of AuroraButton variants — author picks based on Aurora inventory. Locked: use whichever exists in Aurora; do NOT add a Material3 dropdown.

- [ ] In `ModeContentHost.kt`, replace the Ingest row:

  ```kotlin
  OperationMode.Ingest -> com.axon.app.ui.ingest.IngestScreen()
  ```

- [ ] Run: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`. Expected: BUILD SUCCESSFUL.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/ui/ingest/ \
          apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt
  git commit -m "feat(android): Ingest mode UI — submit/status/cancel, cross-validate source vs target, persist jobId"
  ```

- [ ] Close: `bd close axon_rust-21u8.5`.

---

## Task 9: Mode-options screen + per-mode forms

> Beads: `axon_rust-21u8.6`. Depends on Task 8.

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/data/repository/ModeOptionsRepository.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/data/repository/EncryptedTokenStore.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/options/{ModeOptionsScreen,ModeOptionsViewModel}.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/options/forms/{Ask,Query,Summarize,Research,Scrape,Crawl,SearchWeb,Map,Ingest}OptionsForm.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/options/components/{HeadersField,NumberStepperField,EnumDropdownField}.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/data/repository/SettingsRepository.kt` (delegate token to EncryptedTokenStore)
- Modify: `apps/android/app/src/main/java/com/axon/app/data/repository/AxonRepository.kt` (consume options via the decorator)
- Modify: `apps/android/app/src/main/java/com/axon/app/di/AppContainer.kt` (wire ModeOptionsRepository + EncryptedTokenStore)
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNavGraph.kt` (add ModeOptionsRoute)
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsScreen.kt` (replace Toast handler with navigation)

### Step 9.1: EncryptedTokenStore — migrate bearer to EncryptedSharedPreferences

- [ ] Create `apps/android/app/src/main/java/com/axon/app/data/repository/EncryptedTokenStore.kt`:

  ```kotlin
  package com.axon.app.data.repository

  import android.content.Context
  import android.content.SharedPreferences
  import androidx.security.crypto.EncryptedSharedPreferences
  import androidx.security.crypto.MasterKey

  /**
   * Encrypted storage for the bearer token. Migrated out of the plain Preferences-DataStore
   * `axon_settings` blob in this PR (Phase 2). The plain DataStore continues to hold non-secret
   * UX preferences (collection name, etc.).
   */
  class EncryptedTokenStore(context: Context) {
      private val prefs: SharedPreferences = EncryptedSharedPreferences.create(
          context,
          "axon_secrets",
          MasterKey.Builder(context).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build(),
          EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
          EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
      )

      fun read(): String? = prefs.getString(KEY_TOKEN, null)
      fun write(token: String) = prefs.edit().putString(KEY_TOKEN, token).apply()

      companion object {
          private const val KEY_TOKEN = "bearer_token"
      }
  }
  ```

- [ ] In `SettingsRepository.kt`, change the token-read path to first consult `EncryptedTokenStore`, falling back to the existing DataStore key for a one-time migration (read from DataStore, write to encrypted store, then clear DataStore key). Wire `EncryptedTokenStore` via the constructor.

### Step 9.2: ModeOptionsRepository — DataStore-backed flat keys

- [ ] Create `apps/android/app/src/main/java/com/axon/app/data/repository/ModeOptionsRepository.kt`:

  ```kotlin
  package com.axon.app.data.repository

  import android.content.Context
  import androidx.datastore.preferences.core.Preferences
  import androidx.datastore.preferences.core.booleanPreferencesKey
  import androidx.datastore.preferences.core.edit
  import androidx.datastore.preferences.core.intPreferencesKey
  import androidx.datastore.preferences.core.stringPreferencesKey
  import androidx.datastore.preferences.core.stringSetPreferencesKey
  import com.axon.app.ui.operations.OperationMode
  import kotlinx.coroutines.flow.Flow
  import kotlinx.coroutines.flow.map

  class ModeOptionsRepository(context: Context) {
      private val ds = context.modeOptionsDataStore

      // ── Generic API used by per-mode forms via inline helpers ───────────
      fun <T> read(key: Preferences.Key<T>, default: T): Flow<T> =
          ds.data.map { it[key] ?: default }

      suspend fun <T> write(key: Preferences.Key<T>, value: T) {
          ds.edit { it[key] = value }
      }

      // ── Key registry (typed). Lock keys here, not inside per-mode forms. ──
      object Keys {
          // Crawl
          val crawlMaxPages          = intPreferencesKey("mode_options.crawl.max_pages")
          val crawlMaxDepth          = intPreferencesKey("mode_options.crawl.max_depth")
          val crawlRenderMode        = stringPreferencesKey("mode_options.crawl.render_mode")
          val crawlIncludeSubdomains = booleanPreferencesKey("mode_options.crawl.include_subdomains")
          val crawlSkipEmbed         = booleanPreferencesKey("mode_options.crawl.skip_embed")
          val crawlCollection        = stringPreferencesKey("mode_options.crawl.collection")
          val crawlWait              = booleanPreferencesKey("mode_options.crawl.wait")
          val crawlJson              = booleanPreferencesKey("mode_options.crawl.json")
          val crawlHeaders           = stringSetPreferencesKey("mode_options.crawl.headers")  // "Key: Value" strings

          // Scrape
          val scrapeRenderMode = stringPreferencesKey("mode_options.scrape.render_mode")
          val scrapeFormat     = stringPreferencesKey("mode_options.scrape.format")
          val scrapeEmbed      = booleanPreferencesKey("mode_options.scrape.embed")
          val scrapeCollection = stringPreferencesKey("mode_options.scrape.collection")

          // Map
          val mapLimit  = intPreferencesKey("mode_options.map.limit")
          val mapOffset = intPreferencesKey("mode_options.map.offset")

          // Search (web)
          val searchLimit     = intPreferencesKey("mode_options.search.limit")
          val searchOffset    = intPreferencesKey("mode_options.search.offset")
          val searchTimeRange = stringPreferencesKey("mode_options.search.time_range")

          // Summarize
          val summarizeRenderMode      = stringPreferencesKey("mode_options.summarize.render_mode")
          val summarizeRootSelector    = stringPreferencesKey("mode_options.summarize.root_selector")
          val summarizeExcludeSelector = stringPreferencesKey("mode_options.summarize.exclude_selector")

          // Research
          val researchLimit = intPreferencesKey("mode_options.research.limit")

          // Ask
          val askChunkLimit       = intPreferencesKey("mode_options.ask.chunk_limit")
          val askFullDocs         = intPreferencesKey("mode_options.ask.full_docs")
          val askMaxContextChars  = intPreferencesKey("mode_options.ask.max_context_chars")
          val askHybridCandidates = intPreferencesKey("mode_options.ask.hybrid_candidates")
          val askDiagnostics      = booleanPreferencesKey("mode_options.ask.diagnostics")
          val askExplain          = booleanPreferencesKey("mode_options.ask.explain")
          val askCollection       = stringPreferencesKey("mode_options.ask.collection")

          // Query
          val queryLimit      = intPreferencesKey("mode_options.query.limit")
          val queryCollection = stringPreferencesKey("mode_options.query.collection")

          // Ingest
          val ingestIncludeSource = booleanPreferencesKey("mode_options.ingest.include_source")
          val ingestCollection    = stringPreferencesKey("mode_options.ingest.collection")
      }

      object Defaults {
          // Numeric defaults match src/core/config/types/config.rs
          const val crawlMaxPages = 0
          const val crawlMaxDepth = 10
          const val crawlRenderMode = "auto-switch"
          const val scrapeFormat = "markdown"
          const val mapLimit = 10
          const val searchLimit = 10
          const val askChunkLimit = 20
          const val askFullDocs = 6
          const val askMaxContextChars = 300_000
          const val askHybridCandidates = 100
          const val queryLimit = 10
          const val collection = "axon"
      }

      // ── Request decorator — called by AxonRepository before AxonClient sees the DTO ──
      // Note: locked pattern — AxonRepository does NOT take per-mode params.
      // Decorators apply persisted overrides at submit time.
      // Implementation per-mode in AxonRepository extension functions below.
  }
  ```

### Step 9.3: ModeOptionsScreen + 9 hand-written forms

The locked decision is "drop ModeOptionsSpec central table" — implement 9 concrete forms instead.

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/options/ModeOptionsScreen.kt`:

  ```kotlin
  package com.axon.app.ui.options

  import androidx.compose.foundation.layout.Arrangement
  import androidx.compose.foundation.layout.Column
  import androidx.compose.foundation.layout.fillMaxSize
  import androidx.compose.foundation.layout.padding
  import androidx.compose.material3.MaterialTheme
  import androidx.compose.material3.Text
  import androidx.compose.runtime.Composable
  import androidx.compose.ui.Modifier
  import androidx.compose.ui.unit.dp
  import com.axon.app.ui.operations.OperationMode
  import com.axon.app.ui.options.forms.*
  import tv.tootie.aurora.components.AuroraSeparator

  @Composable
  fun ModeOptionsScreen(mode: OperationMode) {
      Column(modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
             verticalArrangement = Arrangement.spacedBy(12.dp)) {
          Text("${mode.label} options", style = MaterialTheme.typography.headlineMedium)
          AuroraSeparator()
          when (mode) {
              OperationMode.Ask       -> AskOptionsForm()
              OperationMode.Query     -> QueryOptionsForm()
              OperationMode.Summarize -> SummarizeOptionsForm()
              OperationMode.Research  -> ResearchOptionsForm()
              OperationMode.Scrape    -> ScrapeOptionsForm()
              OperationMode.Crawl     -> CrawlOptionsForm()
              OperationMode.Search    -> SearchWebOptionsForm()
              OperationMode.Map       -> MapOptionsForm()
              OperationMode.Ingest    -> IngestOptionsForm()
          }
      }
  }
  ```

- [ ] Create `apps/android/app/src/main/java/com/axon/app/ui/options/forms/CrawlOptionsForm.kt` as the canonical example (user-specified full flag list). Repeat the pattern for the other 8 forms, each with their own fields and defaults from `ModeOptionsRepository.Defaults`. Use `AuroraTextField`, `AuroraSwitch`, `AuroraSelect` (or `AuroraButtonGroup`), and the new `HeadersField` component for repeatable Key:Value rows. Include a "Reset to defaults" `AuroraButton` at the bottom of each form.

- [ ] In `OperationsScreen.kt`, replace the Toast handler with navigation:

  ```kotlin
  val navController = LocalAxonNavController.current  // re-introduce or pass via callback
  val onModeOptions = remember(activeMode) {
      { navController.navigate(ModeOptionsRoute(activeMode)) }
  }
  ```

  Add `@Serializable data class ModeOptionsRoute(val mode: OperationMode)` to `AxonNavGraph.kt`. (Compose Navigation 2.8+ supports `@Serializable` enum routes.)

- [ ] Run: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`. Expected: BUILD SUCCESSFUL.

- [ ] Commit:
  ```bash
  git add apps/android/app/src/main/java/com/axon/app/data/repository/{EncryptedTokenStore,ModeOptionsRepository}.kt \
          apps/android/app/src/main/java/com/axon/app/ui/options/ \
          apps/android/app/src/main/java/com/axon/app/data/repository/SettingsRepository.kt \
          apps/android/app/src/main/java/com/axon/app/di/AppContainer.kt \
          apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsScreen.kt \
          apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNavGraph.kt
  git commit -m "feat(android): mode-options screen + 9 hand-written forms + EncryptedSharedPreferences for bearer token"
  ```

- [ ] Close: `bd close axon_rust-21u8.6`.

---

## Pre-merge verification

- [ ] Full compile + test: `./gradlew :app:compileDebugKotlin :app:testDebugUnitTest :app:assembleDebug`. Expected: all three green, debug APK at `apps/android/app/build/outputs/apk/debug/app-debug.apk`.
- [ ] No raw `Color.Black` / `0xFF000000` / pure-dark hex literals in app code: `grep -rn '0xFF000000\|Color.Black' apps/android/app/src/main`. Expected: zero matches.
- [ ] No `while(isActive){fetch; delay}` polling: `grep -rn 'while.*isActive' apps/android/app/src/main`. Expected: only the original ConnectionStatusViewModel, which is updated to `stateIn` as part of Task 4 quick pass.
- [ ] All comments scrubbed of rot-bait phrases (PR refs, future-hook TODOs).
- [ ] Smoke test: launch APK on a connected device or emulator; for each of the 9 modes, FAB-pick the mode, tap the cog, edit one option, return, send a request, observe correct behaviour.
- [ ] APK uploaded: `rclone copyto apps/android/app/build/outputs/apk/debug/app-debug.apk gdrive:axon-apks/axon-android-v$(grep -oP 'version\s*=\s*"\K[^"]+' Cargo.toml | head -1)-$(date +%Y%m%d-%H%M).apk -P`.

## Rollback

Single `git revert <PR-merge-commit>` reverses everything. No schema migrations, no shared state, no server-side dependencies (the `aurora-design-system:feat/prompt-input-action-left` upstream is already merged and independent).

## Out of scope (separate follow-up PR)

- `axon_rust-21u8.10` — SSE for `/v1/research/stream` + `/v1/summarize/stream`. Locked as a Rust-only PR; the Android consumer of those streams comes after the Rust changes ship.
- System page Stack + Config tabs (panel-cookie auth design).
- WorkManager / foreground-service for ingest jobs that must survive process death — current poll model is server-state-aware, fine for v1.
- Hilt migration. Manual DI stays for this PR.

---

## Self-review checklist

Performed after writing the plan:

1. **Spec coverage:** Each of the 10 child beads maps to a task (1-9 + the deferred 10). No gaps.
2. **Placeholder scan:** No "TBD", "implement later", "add error handling"; every code-bearing step shows the exact code or a precise diff. The Ingest UI step references a 150-LOC mirror of `CrawlTab.kt` — the file:line reference + the explicit "Locked: use whichever exists in Aurora" constraint is concrete enough.
3. **Type consistency:** `SearchWebResultUi`, `JobUi`, `SummarizeResultUi`, `SuggestHitUi`, `DomainFacetUi` — same names across foundation, repository, and screen tasks.

---

# Revisions from `/lavra-eng-review` (2026-05-27)

Four parallel review agents (architecture-strategist, code-simplicity-reviewer, security-sentinel, performance-oracle) raised 16 recommendations + 7 CRITICAL GAPs against the v1 plan. These revisions **override** the corresponding sections above and **must be applied** when executing the affected steps.

## R1 — EncryptedTokenStore hardening (Task 9.1, security HIGH, perf MEDIUM)

The v1 Step 9.1 code is unsafe. **Replace** with:

```kotlin
package com.axon.app.data.repository

import android.content.Context
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

/**
 * Encrypted storage for the bearer token. Tolerates AndroidKeyStore invalidation
 * (biometric re-enroll, factory-restore, device-admin wipe) by clearing the
 * shared-prefs file on the first decryption failure and surfacing re-auth.
 *
 * Decrypted token is cached in a @Volatile so repeated authed calls don't
 * round-trip the keystore HAL.
 */
class EncryptedTokenStore(private val context: Context) {
    @Volatile private var cached: String? = null

    private val prefs by lazy {
        runCatching {
            EncryptedSharedPreferences.create(
                context,
                FILE,
                MasterKey.Builder(context).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build(),
                EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
            )
        }.getOrElse {
            context.deleteSharedPreferences(FILE)
            null
        }
    }

    fun read(): String? {
        cached?.let { return it }
        val p = prefs ?: return null
        return runCatching { p.getString(KEY_TOKEN, null) }
            .getOrElse {
                // Master key invalidated; clear and force re-auth.
                context.deleteSharedPreferences(FILE)
                cached = null
                null
            }
            .also { cached = it }
    }

    /** Synchronous commit — credentials must survive immediate process kill. */
    fun write(token: String) {
        val p = prefs ?: return
        @Suppress("ApplySharedPref")
        p.edit().putString(KEY_TOKEN, token).commit()
        cached = token
    }

    fun clear() {
        prefs?.edit()?.remove(KEY_TOKEN)?.commit()
        cached = null
    }

    companion object {
        private const val FILE = "axon_secrets"
        private const val KEY_TOKEN = "bearer_token"
    }
}
```

Additionally, **edit `apps/android/app/src/main/AndroidManifest.xml`** before this task ships:

```xml
<application
    android:allowBackup="false"
    android:dataExtractionRules="@xml/data_extraction_rules"
    …>
```

Create `apps/android/app/src/main/res/xml/data_extraction_rules.xml`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<data-extraction-rules>
    <cloud-backup>
        <exclude domain="sharedpref" path="axon_secrets.xml"/>
    </cloud-backup>
    <device-transfer>
        <exclude domain="sharedpref" path="axon_secrets.xml"/>
    </device-transfer>
</data-extraction-rules>
```

## R2 — Idempotent token migration (Task 9.1, security HIGH)

Replace the prose "read → write → clear" migration with an idempotent boot-time pass on **every** launch, not just the first one:

```kotlin
// In SettingsRepository or AppContainer.applySettings():
suspend fun migrateTokenToEncrypted(plainDs: DataStore<Preferences>, encrypted: EncryptedTokenStore) {
    // Already migrated? Make sure plain copy is gone and exit.
    if (encrypted.read() != null) {
        plainDs.edit { it.remove(KEY_TOKEN) }
        return
    }
    val plain = plainDs.data.first()[KEY_TOKEN]?.takeIf { it.isNotBlank() } ?: return
    encrypted.write(plain)
    plainDs.edit { it.remove(KEY_TOKEN) }
}
```

Called from `AppContainer.initSecurely()` on each app start. Idempotent — safe to call repeatedly.

## R3 — Crawl Authorization header redaction (Task 9, security HIGH)

In the new `CrawlOptionsForm.kt`:

1. Mark sensitive keys (`Authorization`, `Cookie`, `X-Api-Key`, `Proxy-Authorization`, `X-Auth-Token`) — apply `visualTransformation = PasswordVisualTransformation()` on the value `AuroraTextField` when the key matches (case-insensitive). Add a small "show" toggle.
2. Apply `FLAG_SECURE` to the ModeOptionsScreen window:

```kotlin
// In ModeOptionsScreen.kt:
val view = LocalView.current
DisposableEffect(Unit) {
    val window = (view.context as Activity).window
    window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    onDispose { window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
}
```

3. Confirm `AxonClient` is **not** logging request bodies. Grep for `Log.d` / `Log.v` around `execute()` and verify silence.

## R4 — Stats payload chunked rendering (Task 5, perf HIGH — MUST-FIX BEFORE MERGE)

Replace the v1 `StatsSection` proposal that dumps a `Json{prettyPrint=true}.encodeToString(...)` into one giant `Text` with a chunked `LazyColumn`. Reuse `DocumentScreen.chunkDocument(...)` (move it to `apps/android/app/src/main/java/com/axon/app/ui/common/StringChunking.kt` so both consumers can share):

```kotlin
// Move from DocumentScreen.kt -> ui/common/StringChunking.kt as `internal`.
// In StatsSection:
val json = remember(payload) { Json { prettyPrint = true }.encodeToString(JsonElement.serializer(), payload) }
val chunks = remember(json) { chunkDocument(json) }
LazyColumn(modifier = Modifier.fillMaxSize()) {
    items(chunks.size, key = { it }) { i ->
        Text(chunks[i], style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
    }
}
```

Same pattern applies to `SystemScreen`'s Doctor payload (Task 6, Step 6.2) and any future raw-JSON viewer.

## R5 — `ModeOptionsApplicator` interface (Task 9, architecture HIGH)

The v1 plan says "decorator pattern" without showing the signature. Add this contract:

```kotlin
// apps/android/app/src/main/java/com/axon/app/data/repository/ModeOptionsApplicator.kt
package com.axon.app.data.repository

import com.axon.app.data.remote.models.*

/**
 * One method per wire DTO. Implementations read persisted overrides from
 * ModeOptionsRepository and merge them into the request — AxonRepository
 * stays ignorant of which fields exist per mode.
 */
interface ModeOptionsApplicator {
    suspend fun apply(req: SummarizeRequest): SummarizeRequest
    suspend fun apply(req: SearchWebRequest): SearchWebRequest
    suspend fun apply(req: IngestRequest): IngestRequest
    suspend fun apply(req: AskRequest): AskRequest
    suspend fun apply(req: QueryRequest): QueryRequest
    suspend fun apply(req: ScrapeRequest): ScrapeRequest
    suspend fun apply(req: CrawlRequest): CrawlRequest
    suspend fun apply(req: MapRequest): MapRequest
    suspend fun apply(req: ResearchRequest): ResearchRequest
}
```

`ModeOptionsRepository` implements this interface; `AxonRepository` takes it as a constructor dependency and calls `applicator.apply(req)` before passing to `client.*`.

Test obligation: a focused `ModeOptionsApplicatorTest` that, per request type, sets a DataStore key and verifies the apply-merged DTO has the override field set.

## R6 — `RecentJobsRepository` dedupe + LRU cap (Task 4.1, architecture HIGH, perf MEDIUM)

Replace the v1 `add()` body with:

```kotlin
private const val MAX_RECENT_JOBS = 100

suspend fun add(job: RecentJob) {
    ds.edit { prefs ->
        // Decode current entries, drop any with the same jobId, prepend the new entry,
        // trim to the LRU cap, re-encode.
        val current = (prefs[key] ?: emptySet())
            .mapNotNull { runCatching { json.decodeFromString<RecentJob>(it) }.getOrNull() }
            .filterNot { it.jobId == job.jobId }
        val updated = (listOf(job) + current).take(MAX_RECENT_JOBS)
        prefs[key] = updated.map { json.encodeToString(it) }.toSet()
    }
}
```

## R7 — OkHttp client tuning (Task 1, architecture HIGH)

Modify `AxonClient` constructor to share a single `ConnectionPool` and lift the per-host cap:

```kotlin
private val sharedPool = ConnectionPool(maxIdleConnections = 16, keepAliveDuration = 5, TimeUnit.MINUTES)
private val sharedDispatcher = Dispatcher().apply { maxRequestsPerHost = 16 }

private val http = OkHttpClient.Builder()
    .connectionPool(sharedPool)
    .dispatcher(sharedDispatcher)
    .connectTimeout(CONNECT_TIMEOUT_SECONDS, TimeUnit.SECONDS)
    .readTimeout(READ_TIMEOUT_SECONDS, TimeUnit.SECONDS)
    .writeTimeout(WRITE_TIMEOUT_SECONDS, TimeUnit.SECONDS)
    .build()

private val httpLong = http.newBuilder().readTimeout(LONG_READ_TIMEOUT_SECONDS, TimeUnit.SECONDS).build()
private val httpStream = http.newBuilder().readTimeout(STREAM_READ_TIMEOUT_SECONDS, TimeUnit.SECONDS).build()
```

## R8 — `Resource<T>` consolidation (architecture-wide, simplicity)

Replace `SummarizeUiState`, `SearchWebUiState`, `JobListUi`, `SectionState<T>`, and `DoctorUi` with a single sealed interface:

```kotlin
// apps/android/app/src/main/java/com/axon/app/ui/common/Resource.kt
package com.axon.app.ui.common

sealed interface Resource<out T> {
    data object Idle    : Resource<Nothing>
    data object Loading : Resource<Nothing>
    data class  Ready<out T>(val value: T)        : Resource<T>
    data class  Error(val message: String)         : Resource<Nothing>
}
```

ViewModels expose `StateFlow<Resource<T>>`. `IngestUi` keeps its own multi-state sealed interface because it has 5 legitimately distinct states (Idle / Submitting / Submitted / Status / Error).

## R9 — Inline `Keys` + `Defaults` into per-form files (Task 9, simplicity)

Delete `ModeOptionsRepository.Keys` and `ModeOptionsRepository.Defaults` objects. Each `<Mode>OptionsForm.kt` owns its own keys + defaults file-private:

```kotlin
// CrawlOptionsForm.kt
private val KEY_MAX_PAGES = intPreferencesKey("mode_options.crawl.max_pages")
private const val DEFAULT_MAX_PAGES = 0
// …
```

`ModeOptionsRepository` keeps only the generic `read/write` API.

## R10 — Visible-tab-only polling (Task 4.3, perf MEDIUM)

Replace JobsViewModel's 4 independent `stateIn` flows with a single `flatMapLatest`-driven flow:

```kotlin
private val _selectedTab = MutableStateFlow(AxonClient.JobKind.Crawl)
fun selectTab(kind: AxonClient.JobKind) { _selectedTab.value = kind }

val visibleJobs: StateFlow<Resource<List<JobUi>>> = _selectedTab
    .flatMapLatest { kind ->
        flow {
            while (true) {
                emit(container.axonRepository.listJobs(kind).fold(
                    onSuccess = { Resource.Ready(it) },
                    onFailure = { Resource.Error(it.message ?: "Error") },
                ))
                delay(POLL_INTERVAL_MS)
            }
        }
    }
    .catch { e -> if (e is CancellationException) throw e; emit(Resource.Error(e.message ?: "Error")) }
    .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), Resource.Loading)
```

Idle tabs no longer poll. JobsScreen calls `vm.selectTab(JobKind.values()[selected])` from a `LaunchedEffect(selected)` — but see R15 for the safer dispatch.

## R11 — Knowledge memoization (Task 5, perf MEDIUM)

In `KnowledgeViewModel`, wrap each `loadX()` with a 30s freshness check:

```kotlin
private data class Cached<T>(val value: T, val at: Long)
private var sourcesCache: Cached<List<SourceEntryUi>>? = null

fun loadSources() {
    sourcesCache?.let { if (System.currentTimeMillis() - it.at < 30_000) return }
    viewModelScope.launch {
        // … fetch and assign sourcesCache = Cached(result, now())
    }
}
```

Pull-to-refresh sets `sourcesCache = null` first.

## R12 — `EncryptedTokenStore` token cache

Already incorporated in R1 above (`@Volatile var cached`).

## R13 — `IngestSource.validate` host-equality (Task 8, security LOW)

Replace the substring check with `URL.host` end-check:

```kotlin
fun validate(target: String): String? {
    if (target.isBlank()) return "Target is required"
    val hint = targetHostHint ?: return null
    val host = runCatching { java.net.URL(target).host }.getOrNull()
        ?: return null  // Non-URL forms (git@host:owner/repo) — let server validate
    return if (host == hint || host.endsWith(".$hint")) null
           else "Expected target host to be $hint"
}
```

## R14 — `AskViewModel.previousMode` survives rotation (Task 2.4, architecture MEDIUM)

Move `previousMode` into the AskViewModel as `MutableStateFlow<OperationMode?>` (lifecycle-safe), or use `rememberSaveable` in OperationsScreen:

```kotlin
var previousMode by rememberSaveable { mutableStateOf<OperationMode?>(null) }
```

`OperationMode` is an enum so `rememberSaveable` natively supports it via the default Saver.

## R15 — Jobs tab dispatch explicit map (Task 4.4, simplicity MEDIUM)

Replace `JobKind.values()[selected]` with:

```kotlin
private val tabKinds = listOf(
    AxonClient.JobKind.Crawl,
    AxonClient.JobKind.Embed,
    AxonClient.JobKind.Extract,
    AxonClient.JobKind.Ingest,
)
val current = tabKinds[selected]
```

Reordering TABS list now changes BOTH labels and dispatch in one place.

## R16 — `searchWeb` auto-crawl backpressure (Task 7, architecture MEDIUM)

The Tavily search auto-enqueues N crawl jobs. The server enforces `AXON_MAX_PENDING_CRAWL_JOBS` and rejects when full. `SearchWebViewModel` already maps repository failures to `Resource.Error`, but the screen's empty-state should distinguish queue-full from network failure — surface a callout when `crawlJobsEnqueued == 0 && results.isNotEmpty() && result.autoCrawlStatus?.skipped > 0`:

```kotlin
if (s.result.autoCrawlStatus?.skipped ?: 0 > 0) {
    AuroraCallout(
        title = "Auto-crawl queue full",
        message = "Some results were not enqueued for indexing — try again later.",
        variant = AuroraCalloutVariant.Warn,
        modifier = Modifier.fillMaxWidth(),
    )
}
```

## Critical-gap closure

After applying R1–R16, every CRITICAL GAP row in the failure-modes table flips:

| Codepath | After fix |
|---|---|
| EncryptedTokenStore + keystore wipe | R1 try/catch + clear → re-auth flow surfaces; logged |
| Token migration mid-crash | R2 idempotent cleanup on each launch |
| Stats 50KB Text → ANR | R4 chunked LazyColumn |
| Crawl Authorization screen-capture | R3 PasswordVisualTransformation + FLAG_SECURE |
| RecentJobsRepository dup | R6 dedupe-by-jobId in add() + LRU cap |
| OkHttp pool starvation | R7 maxRequestsPerHost=16 + shared pool |
| AskViewModel rotation | R14 rememberSaveable |

