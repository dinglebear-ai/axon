# Axon Android App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a native Android app at `apps/android/` in the axon_rust repo that gives mobile access to the Axon RAG API — Ask (Q&A), Search (vector query), Sources (index browser), and Settings.

**Architecture:** Single-Activity Jetpack Compose app with bottom-nav routing to 4 screens (Ask, Search, Sources, Settings). MVVM with manual DI via AppContainer. OkHttp for HTTP + kotlinx.serialization for JSON. Room for ask history persistence, DataStore for settings. Aurora design system pulled in via Gradle composite build pointing at `../../../aurora-design-system/android`.

**Tech Stack:** Kotlin 2.1.0, Jetpack Compose (BOM 2026.04.01), Navigation Compose 2.8.5, OkHttp 4.12, kotlinx.serialization-json 1.7.3, Room 2.6.1 (KSP), DataStore Preferences 1.1.2, Aurora design system (composite build), compileSdk 36 / minSdk 24.

---

## File Map

```
apps/android/
├── settings.gradle.kts                              # composite build + module declarations
├── build.gradle.kts                                 # root build (no source, just plugin classpaths)
├── gradle/
│   ├── libs.versions.toml                           # version catalog
│   └── wrapper/
│       ├── gradle-wrapper.jar                       # copied from aurora-design-system/android
│       └── gradle-wrapper.properties                # Gradle 8.10
├── gradlew                                          # copied from aurora-design-system/android
├── gradlew.bat                                      # copied from aurora-design-system/android
├── local.properties                                 # ANDROID_HOME path
└── app/
    ├── build.gradle.kts                             # app module deps + KSP room compiler
    └── src/main/
        ├── AndroidManifest.xml
        ├── res/xml/network_security_config.xml      # allow *.ts.net Tailscale hosts
        ├── res/values/strings.xml
        └── java/com/axon/app/
            ├── AxonApp.kt                           # Application; builds AppContainer
            ├── MainActivity.kt                      # setContent + AuroraTheme + NavHost
            ├── ui/
            │   ├── theme/AxonTheme.kt               # thin AuroraTheme wrapper
            │   ├── nav/AxonNavGraph.kt              # NavHost + bottom bar
            │   ├── ask/
            │   │   ├── AskScreen.kt                 # prompt input + streaming answer + history list
            │   │   └── AskViewModel.kt              # UiState: Idle/Loading/Success/Error
            │   ├── search/
            │   │   ├── SearchScreen.kt              # query input + LazyColumn of QueryHit cards
            │   │   └── SearchViewModel.kt
            │   ├── sources/
            │   │   ├── SourcesScreen.kt             # LazyColumn of (url, chunks) rows
            │   │   └── SourcesViewModel.kt
            │   └── settings/
            │       ├── SettingsScreen.kt            # server URL, token, collection, test-connection
            │       └── SettingsViewModel.kt
            ├── data/
            │   ├── remote/
            │   │   ├── AxonModels.kt                # all @Serializable request/response classes
            │   │   └── AxonClient.kt                # OkHttpClient + suspend fns per endpoint
            │   ├── local/
            │   │   ├── AskHistoryEntry.kt           # Room @Entity
            │   │   ├── AskHistoryDao.kt             # Room @Dao
            │   │   └── AppDatabase.kt               # Room @Database
            │   └── repository/
            │       ├── AxonRepository.kt            # ask, query, sources, stats — returns Result<T>
            │       └── SettingsRepository.kt        # DataStore read/write for serverUrl/token/collection
            └── di/
                └── AppContainer.kt                  # manual DI root
```

---

## Task 1: Gradle Project Scaffold

**Files:**
- Create: `apps/android/settings.gradle.kts`
- Create: `apps/android/build.gradle.kts`
- Create: `apps/android/gradle/libs.versions.toml`
- Create: `apps/android/gradle/wrapper/gradle-wrapper.properties`
- Create: `apps/android/local.properties`

- [ ] **Step 1: Copy gradle wrapper binaries from aurora-design-system**

```bash
cp /home/jmagar/workspace/aurora-design-system/android/gradlew apps/android/gradlew
cp /home/jmagar/workspace/aurora-design-system/android/gradlew.bat apps/android/gradlew.bat
cp -r /home/jmagar/workspace/aurora-design-system/android/gradle/wrapper apps/android/gradle/wrapper
chmod +x apps/android/gradlew
```

- [ ] **Step 2: Create `apps/android/gradle/wrapper/gradle-wrapper.properties`**

```properties
distributionBase=GRADLE_USER_HOME
distributionPath=wrapper/dists
distributionUrl=https\://services.gradle.org/distributions/gradle-8.10-bin.zip
networkTimeout=10000
validateDistributionUrl=true
zipStoreBase=GRADLE_USER_HOME
zipStorePath=wrapper/dists
```

- [ ] **Step 3: Create `apps/android/gradle/libs.versions.toml`**

```toml
[versions]
kotlin = "2.1.0"
agp = "8.7.0"
ksp = "2.1.0-1.0.29"
composeBom = "2026.04.01"
activity-compose = "1.9.3"
navigation = "2.8.5"
lifecycle = "2.8.7"
datastore = "1.1.2"
room = "2.6.1"
okhttp = "4.12.0"
kotlinx-coroutines = "1.8.1"
kotlinx-serialization = "1.7.3"
kotlinx-collections-immutable = "0.3.8"
junit = "4.13.2"
mockwebserver = "4.12.0"

[plugins]
android-application  = { id = "com.android.application",                      version.ref = "agp" }
kotlin-android       = { id = "org.jetbrains.kotlin.android",                  version.ref = "kotlin" }
compose-compiler     = { id = "org.jetbrains.kotlin.plugin.compose",           version.ref = "kotlin" }
kotlinx-serialization = { id = "org.jetbrains.kotlin.plugin.serialization",   version.ref = "kotlin" }
ksp                  = { id = "com.google.devtools.ksp",                        version.ref = "ksp" }

[libraries]
compose-bom                      = { group = "androidx.compose",           name = "compose-bom",                    version.ref = "composeBom" }
compose-ui                       = { group = "androidx.compose.ui",        name = "ui" }
compose-ui-tooling               = { group = "androidx.compose.ui",        name = "ui-tooling" }
compose-ui-tooling-preview       = { group = "androidx.compose.ui",        name = "ui-tooling-preview" }
compose-material3                = { group = "androidx.compose.material3", name = "material3" }
compose-material-icons-extended  = { group = "androidx.compose.material",  name = "material-icons-extended" }
activity-compose                 = { group = "androidx.activity",           name = "activity-compose",               version.ref = "activity-compose" }
navigation-compose               = { group = "androidx.navigation",         name = "navigation-compose",             version.ref = "navigation" }
lifecycle-viewmodel-compose      = { group = "androidx.lifecycle",          name = "lifecycle-viewmodel-compose",    version.ref = "lifecycle" }
lifecycle-runtime-compose        = { group = "androidx.lifecycle",          name = "lifecycle-runtime-compose",      version.ref = "lifecycle" }
datastore-preferences            = { group = "androidx.datastore",          name = "datastore-preferences",          version.ref = "datastore" }
room-runtime                     = { group = "androidx.room",               name = "room-runtime",                   version.ref = "room" }
room-ktx                         = { group = "androidx.room",               name = "room-ktx",                      version.ref = "room" }
room-compiler                    = { group = "androidx.room",               name = "room-compiler",                  version.ref = "room" }
okhttp                           = { group = "com.squareup.okhttp3",        name = "okhttp",                         version.ref = "okhttp" }
kotlinx-coroutines-android       = { group = "org.jetbrains.kotlinx",       name = "kotlinx-coroutines-android",     version.ref = "kotlinx-coroutines" }
kotlinx-serialization-json       = { group = "org.jetbrains.kotlinx",       name = "kotlinx-serialization-json",     version.ref = "kotlinx-serialization" }
kotlinx-collections-immutable    = { group = "org.jetbrains.kotlinx",       name = "kotlinx-collections-immutable",  version.ref = "kotlinx-collections-immutable" }
junit                            = { group = "junit",                        name = "junit",                          version.ref = "junit" }
mockwebserver                    = { group = "com.squareup.okhttp3",        name = "mockwebserver",                  version.ref = "mockwebserver" }
```

- [ ] **Step 4: Create `apps/android/settings.gradle.kts`**

```kotlin
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

// Pull in aurora library from sibling repo via composite build.
// Relative path: apps/android → workspace/aurora-design-system/android
includeBuild("../../../aurora-design-system/android") {
    dependencySubstitution {
        substitute(module("tv.tootie.aurora:aurora")).using(project(":aurora"))
    }
}

rootProject.name = "axon-android"
include(":app")
```

- [ ] **Step 5: Create `apps/android/build.gradle.kts`**

```kotlin
// Top-level build file — no source here. Plugin versions come from the version catalog.
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.android)       apply false
    alias(libs.plugins.compose.compiler)     apply false
    alias(libs.plugins.kotlinx.serialization) apply false
    alias(libs.plugins.ksp)                  apply false
}
```

- [ ] **Step 6: Create `apps/android/local.properties`**

```properties
sdk.dir=/home/jmagar/Android/Sdk
```

- [ ] **Step 7: Verify Gradle sync resolves**

```bash
cd apps/android && ./gradlew tasks --quiet 2>&1 | tail -20
```
Expected: task list printed without BUILD FAILED.

---

## Task 2: App Module Build + Manifest

**Files:**
- Create: `apps/android/app/build.gradle.kts`
- Create: `apps/android/app/src/main/AndroidManifest.xml`
- Create: `apps/android/app/src/main/res/xml/network_security_config.xml`
- Create: `apps/android/app/src/main/res/values/strings.xml`

- [ ] **Step 1: Create `apps/android/app/build.gradle.kts`**

```kotlin
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.kotlinx.serialization)
    alias(libs.plugins.ksp)
}

android {
    namespace = "com.axon.app"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.axon.app"
        minSdk = 24
        targetSdk = 36
        versionCode = 1
        versionName = "1.0"
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    // Aurora design system (resolved via composite build in settings.gradle.kts)
    implementation("tv.tootie.aurora:aurora")

    // Compose
    val bom = platform(libs.compose.bom)
    implementation(bom)
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons.extended)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)

    // Navigation + Lifecycle
    implementation(libs.activity.compose)
    implementation(libs.navigation.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)

    // Persistence
    implementation(libs.datastore.preferences)
    implementation(libs.room.runtime)
    implementation(libs.room.ktx)
    ksp(libs.room.compiler)

    // Network + JSON
    implementation(libs.okhttp)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.collections.immutable)

    // Tests
    testImplementation(libs.junit)
    testImplementation(libs.mockwebserver)
}
```

- [ ] **Step 2: Create `apps/android/app/src/main/AndroidManifest.xml`**

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <uses-permission android:name="android.permission.INTERNET" />

    <application
        android:name=".AxonApp"
        android:label="@string/app_name"
        android:networkSecurityConfig="@xml/network_security_config"
        android:theme="@style/Theme.AppCompat.DayNight.NoActionBar">

        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:windowSoftInputMode="adjustResize">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>

    </application>

</manifest>
```

- [ ] **Step 3: Create `apps/android/app/src/main/res/xml/network_security_config.xml`**

```xml
<?xml version="1.0" encoding="utf-8"?>
<network-security-config>
    <!-- Allow cleartext HTTP for Tailscale MagicDNS hostnames and IPs -->
    <domain-config cleartextTrafficPermitted="true">
        <domain includeSubdomains="true">ts.net</domain>
        <domain includeSubdomains="true">tailvpn.net</domain>
    </domain-config>
</network-security-config>
```

- [ ] **Step 4: Create `apps/android/app/src/main/res/values/strings.xml`**

```xml
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <string name="app_name">Axon</string>
    <string name="nav_ask">Ask</string>
    <string name="nav_search">Search</string>
    <string name="nav_sources">Sources</string>
    <string name="nav_settings">Settings</string>
</resources>
```

- [ ] **Step 5: Verify module compiles**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -30
```
Expected: BUILD SUCCESSFUL (no Kotlin files yet, but module should resolve).

---

## Task 3: Data Models + HTTP Client

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/data/remote/AxonModels.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt`
- Create: `apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientTest.kt`

- [ ] **Step 1: Write the failing test**

Create `apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientTest.kt`:

```kotlin
package com.axon.app.data.remote

import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class AxonClientTest {

    private lateinit var server: MockWebServer
    private lateinit var client: AxonClient

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
        client = AxonClient(
            baseUrl = server.url("/").toString().trimEnd('/'),
            token = "test-token",
        )
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    @Test
    fun `healthz returns true when server responds 200`() = runBlocking {
        server.enqueue(MockResponse().setBody("ok").setResponseCode(200))
        val healthy = client.healthz()
        assertTrue(healthy)
        val req = server.takeRequest()
        assertEquals("/healthz", req.path)
    }

    @Test
    fun `ask sends auth header and deserializes response`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"query":"hello","answer":"world","timing_ms":{"total_ms":500}}""")
                .addHeader("Content-Type", "application/json")
        )
        val result = client.ask(AskRequest(query = "hello"))
        assertTrue(result.isSuccess)
        assertEquals("world", result.getOrThrow().answer)
        val req = server.takeRequest()
        assertEquals("Bearer test-token", req.getHeader("Authorization"))
        assertEquals("/v1/ask", req.path)
    }

    @Test
    fun `query deserializes results list`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"results":[{"rank":1,"score":0.9,"rerank_score":0.0,"url":"https://a.com","source":"a.com","snippet":"some text","chunk_index":null}]}""")
                .addHeader("Content-Type", "application/json")
        )
        val result = client.query(QueryRequest(query = "test"))
        assertTrue(result.isSuccess)
        assertEquals(1, result.getOrThrow().results.size)
        assertEquals("https://a.com", result.getOrThrow().results[0].url)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd apps/android && ./gradlew :app:testDebugUnitTest --tests "com.axon.app.data.remote.AxonClientTest" 2>&1 | tail -20
```
Expected: FAILED — AxonClient, AskRequest, QueryRequest not yet defined.

- [ ] **Step 3: Create `apps/android/app/src/main/java/com/axon/app/data/remote/AxonModels.kt`**

```kotlin
package com.axon.app.data.remote

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

// ── Requests ──────────────────────────────────────────────────────────────────

@Serializable
data class AskRequest(
    val query: String,
    val collection: String? = null,
)

@Serializable
data class QueryRequest(
    val query: String,
    val limit: Int = 10,
    val collection: String? = null,
)

@Serializable
data class SourcesRequest(
    val limit: Int = 50,
    val offset: Int = 0,
    val collection: String? = null,
)

// ── Ask response ──────────────────────────────────────────────────────────────

@Serializable
data class AskResponse(
    val query: String,
    val answer: String,
    @SerialName("timing_ms") val timingMs: AskTiming? = null,
)

@Serializable
data class AskTiming(
    @SerialName("total_ms") val totalMs: Long? = null,
)

// ── Query response ────────────────────────────────────────────────────────────

@Serializable
data class QueryResponse(
    val results: List<QueryHit>,
)

@Serializable
data class QueryHit(
    val rank: Long,
    val score: Double,
    @SerialName("rerank_score") val rerankScore: Double = 0.0,
    val url: String,
    val source: String,
    val snippet: String,
    @SerialName("chunk_index") val chunkIndex: Long? = null,
)

// ── Sources response ──────────────────────────────────────────────────────────
// Rust serializes Vec<(String, usize)> as [[url, count], ...].
// We keep the raw JsonArray and let AxonRepository map it.

@Serializable
data class SourcesResponse(
    val count: Int,
    val limit: Int,
    val offset: Int,
    val urls: JsonArray,
)

// ── Stats ─────────────────────────────────────────────────────────────────────

@Serializable
data class StatsResponse(
    val payload: JsonObject,
)
```

- [ ] **Step 4: Create `apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt`**

```kotlin
package com.axon.app.data.remote

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.TimeUnit

private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

private val json = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
}

class AxonClient(
    private var baseUrl: String,
    private var token: String,
) {
    private val http = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .writeTimeout(15, TimeUnit.SECONDS)
        .build()

    fun updateConfig(newBaseUrl: String, newToken: String) {
        baseUrl = newBaseUrl.trimEnd('/')
        token = newToken
    }

    suspend fun healthz(): Boolean = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder()
                .url("$baseUrl/healthz")
                .get()
                .build()
            http.newCall(req).execute().use { it.isSuccessful }
        }.getOrDefault(false)
    }

    suspend fun ask(request: AskRequest): Result<AskResponse> = withContext(Dispatchers.IO) {
        post("/v1/ask", request)
    }

    suspend fun query(request: QueryRequest): Result<QueryResponse> = withContext(Dispatchers.IO) {
        post("/v1/query", request)
    }

    suspend fun sources(request: SourcesRequest = SourcesRequest()): Result<SourcesResponse> =
        withContext(Dispatchers.IO) {
            get("/v1/sources?limit=${request.limit}&offset=${request.offset}")
        }

    suspend fun stats(): Result<StatsResponse> = withContext(Dispatchers.IO) {
        get("/v1/stats")
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    private inline fun <reified T> authRequest(builder: Request.Builder): Request.Builder =
        builder.header("Authorization", "Bearer $token")
               .header("x-api-key", token)

    private inline fun <reified B, reified R> post(path: String, body: B): Result<R> =
        runCatching {
            val bodyBytes = json.encodeToString(body).toRequestBody(JSON_MEDIA_TYPE)
            val req = authRequest<B>(
                Request.Builder()
                    .url("$baseUrl$path")
                    .post(bodyBytes)
            ).build()
            http.newCall(req).execute().use { resp ->
                if (!resp.isSuccessful) {
                    val msg = resp.body?.string() ?: resp.message
                    error("HTTP ${resp.code}: $msg")
                }
                json.decodeFromString<R>(resp.body!!.string())
            }
        }

    private inline fun <reified R> get(path: String): Result<R> =
        runCatching {
            val req = authRequest<Unit>(Request.Builder().url("$baseUrl$path").get()).build()
            http.newCall(req).execute().use { resp ->
                if (!resp.isSuccessful) {
                    val msg = resp.body?.string() ?: resp.message
                    error("HTTP ${resp.code}: $msg")
                }
                json.decodeFromString<R>(resp.body!!.string())
            }
        }
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd apps/android && ./gradlew :app:testDebugUnitTest --tests "com.axon.app.data.remote.AxonClientTest" 2>&1 | tail -20
```
Expected: 3 tests PASSED.

- [ ] **Step 6: Commit**

```bash
cd apps/android && git add -A && git -C /home/jmagar/workspace/axon_rust add apps/android/ && git -C /home/jmagar/workspace/axon_rust commit -m "feat(android): scaffold gradle project + AxonClient with models"
```

---

## Task 4: Room Database + DataStore + AppContainer DI

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/data/local/AskHistoryEntry.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/data/local/AskHistoryDao.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/data/local/AppDatabase.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/data/repository/SettingsRepository.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/di/AppContainer.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/AxonApp.kt`

- [ ] **Step 1: Create `AskHistoryEntry.kt`**

```kotlin
package com.axon.app.data.local

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "ask_history")
data class AskHistoryEntry(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val query: String,
    val answer: String,
    @ColumnInfo(name = "asked_at") val askedAt: Long = System.currentTimeMillis(),
)
```

- [ ] **Step 2: Create `AskHistoryDao.kt`**

```kotlin
package com.axon.app.data.local

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import kotlinx.coroutines.flow.Flow

@Dao
interface AskHistoryDao {
    @Query("SELECT * FROM ask_history ORDER BY asked_at DESC LIMIT 50")
    fun recent(): Flow<List<AskHistoryEntry>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(entry: AskHistoryEntry)

    @Query("DELETE FROM ask_history")
    suspend fun clearAll()
}
```

- [ ] **Step 3: Create `AppDatabase.kt`**

```kotlin
package com.axon.app.data.local

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase

@Database(entities = [AskHistoryEntry::class], version = 1, exportSchema = false)
abstract class AppDatabase : RoomDatabase() {
    abstract fun askHistoryDao(): AskHistoryDao

    companion object {
        fun build(context: Context): AppDatabase =
            Room.databaseBuilder(context, AppDatabase::class.java, "axon.db")
                .fallbackToDestructiveMigration()
                .build()
    }
}
```

- [ ] **Step 4: Create `SettingsRepository.kt`**

```kotlin
package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "settings")

private val KEY_SERVER_URL  = stringPreferencesKey("server_url")
private val KEY_TOKEN       = stringPreferencesKey("token")
private val KEY_COLLECTION  = stringPreferencesKey("collection")

const val DEFAULT_SERVER_URL = "https://axon.tootie.tv"
const val DEFAULT_COLLECTION = "axon"

data class AxonSettings(
    val serverUrl: String = DEFAULT_SERVER_URL,
    val token: String = "",
    val collection: String = DEFAULT_COLLECTION,
)

class SettingsRepository(private val context: Context) {

    val settings: Flow<AxonSettings> = context.dataStore.data.map { prefs ->
        AxonSettings(
            serverUrl  = prefs[KEY_SERVER_URL]  ?: DEFAULT_SERVER_URL,
            token      = prefs[KEY_TOKEN]       ?: "",
            collection = prefs[KEY_COLLECTION]  ?: DEFAULT_COLLECTION,
        )
    }

    suspend fun save(settings: AxonSettings) {
        context.dataStore.edit { prefs ->
            prefs[KEY_SERVER_URL]  = settings.serverUrl
            prefs[KEY_TOKEN]       = settings.token
            prefs[KEY_COLLECTION]  = settings.collection
        }
    }
}
```

- [ ] **Step 5: Create `AppContainer.kt`**

```kotlin
package com.axon.app.di

import android.content.Context
import com.axon.app.data.local.AppDatabase
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.AxonRepository
import com.axon.app.data.repository.SettingsRepository
import com.axon.app.data.repository.DEFAULT_SERVER_URL

class AppContainer(context: Context) {
    val settingsRepository = SettingsRepository(context)
    private val db = AppDatabase.build(context)
    val askHistoryDao = db.askHistoryDao()

    val axonClient = AxonClient(
        baseUrl = DEFAULT_SERVER_URL,
        token = "",
    )

    val axonRepository = AxonRepository(axonClient)
}
```

- [ ] **Step 6: Create `AxonApp.kt`**

```kotlin
package com.axon.app

import android.app.Application
import com.axon.app.di.AppContainer

class AxonApp : Application() {
    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
    }
}
```

- [ ] **Step 7: Compile check**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -30
```
Expected: BUILD SUCCESSFUL — AxonRepository not yet created but the above should compile in isolation.

---

## Task 5: AxonRepository

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/data/repository/AxonRepository.kt`

- [ ] **Step 1: Create `AxonRepository.kt`**

```kotlin
package com.axon.app.data.repository

import com.axon.app.data.remote.AxonClient
import com.axon.app.data.remote.AskRequest
import com.axon.app.data.remote.QueryRequest
import com.axon.app.data.remote.SourcesRequest
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.int

data class AskResultUi(val query: String, val answer: String, val timingMs: Long?)
data class QueryHitUi(val rank: Long, val score: Double, val url: String, val source: String, val snippet: String)
data class SourceEntryUi(val url: String, val chunks: Int)

class AxonRepository(private val client: AxonClient) {

    suspend fun ask(query: String, collection: String? = null): Result<AskResultUi> =
        client.ask(AskRequest(query = query, collection = collection)).map { r ->
            AskResultUi(query = r.query, answer = r.answer, timingMs = r.timingMs?.totalMs)
        }

    suspend fun query(query: String, limit: Int = 10, collection: String? = null): Result<List<QueryHitUi>> =
        client.query(QueryRequest(query = query, limit = limit, collection = collection)).map { r ->
            r.results.map { h ->
                QueryHitUi(rank = h.rank, score = h.score, url = h.url, source = h.source, snippet = h.snippet)
            }
        }

    suspend fun sources(limit: Int = 50, offset: Int = 0): Result<List<SourceEntryUi>> =
        client.sources(SourcesRequest(limit = limit, offset = offset)).map { r ->
            r.urls.mapNotNull { element ->
                runCatching {
                    val arr = element.jsonArray
                    SourceEntryUi(
                        url = arr[0].jsonPrimitive.content,
                        chunks = arr[1].jsonPrimitive.int,
                    )
                }.getOrNull()
            }
        }

    suspend fun ping(): Boolean = client.healthz()
}
```

- [ ] **Step 2: Compile check**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: BUILD SUCCESSFUL.

---

## Task 6: Theme + Navigation + MainActivity

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/theme/AxonTheme.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNavGraph.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/MainActivity.kt`

- [ ] **Step 1: Create `AxonTheme.kt`**

```kotlin
package com.axon.app.ui.theme

import androidx.compose.runtime.Composable
import tv.tootie.aurora.theme.AuroraTheme

@Composable
fun AxonTheme(content: @Composable () -> Unit) {
    AuroraTheme(content = content)
}
```

- [ ] **Step 2: Create `AxonNavGraph.kt`**

```kotlin
package com.axon.app.ui.nav

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.QuestionAnswer
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.navigation.NavDestination.Companion.hasRoute
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.axon.app.ui.ask.AskScreen
import com.axon.app.ui.search.SearchScreen
import com.axon.app.ui.settings.SettingsScreen
import com.axon.app.ui.sources.SourcesScreen
import kotlinx.serialization.Serializable

@Serializable object AskRoute
@Serializable object SearchRoute
@Serializable object SourcesRoute
@Serializable object SettingsRoute

private data class NavItem(
    val label: String,
    val icon: androidx.compose.ui.graphics.vector.ImageVector,
    val route: Any,
)

private val navItems = listOf(
    NavItem("Ask",      Icons.Default.QuestionAnswer, AskRoute),
    NavItem("Search",   Icons.Default.Hub,            SearchRoute),
    NavItem("Sources",  Icons.Default.List,           SourcesRoute),
    NavItem("Settings", Icons.Default.Settings,       SettingsRoute),
)

@Composable
fun AxonNavGraph() {
    val navController = rememberNavController()
    val backStackEntry by navController.currentBackStackEntryAsState()
    val currentDest = backStackEntry?.destination

    Scaffold(
        bottomBar = {
            NavigationBar {
                navItems.forEach { item ->
                    NavigationBarItem(
                        selected = currentDest?.hasRoute(item.route::class) == true,
                        onClick = {
                            navController.navigate(item.route) {
                                popUpTo(navController.graph.findStartDestination().id) {
                                    saveState = true
                                }
                                launchSingleTop = true
                                restoreState = true
                            }
                        },
                        icon = { Icon(item.icon, contentDescription = item.label) },
                        label = { Text(item.label) },
                    )
                }
            }
        }
    ) { innerPadding ->
        NavHost(
            navController = navController,
            startDestination = AskRoute,
            modifier = androidx.compose.ui.Modifier.padding(innerPadding),
        ) {
            composable<AskRoute>     { AskScreen() }
            composable<SearchRoute>  { SearchScreen() }
            composable<SourcesRoute> { SourcesScreen() }
            composable<SettingsRoute>{ SettingsScreen() }
        }
    }
}
```

- [ ] **Step 3: Create `MainActivity.kt`**

```kotlin
package com.axon.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import com.axon.app.ui.nav.AxonNavGraph
import com.axon.app.ui.theme.AxonTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            AxonTheme {
                AxonNavGraph()
            }
        }
    }
}
```

---

## Task 7: Settings Screen

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/settings/SettingsViewModel.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/settings/SettingsScreen.kt`

- [ ] **Step 1: Create `SettingsViewModel.kt`**

```kotlin
package com.axon.app.ui.settings

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.AxonSettings
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

sealed interface ConnectionState {
    object Idle : ConnectionState
    object Testing : ConnectionState
    object Ok : ConnectionState
    data class Failed(val error: String) : ConnectionState
}

class SettingsViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _settings = MutableStateFlow(AxonSettings())
    val settings: StateFlow<AxonSettings> = _settings.asStateFlow()

    private val _connection = MutableStateFlow<ConnectionState>(ConnectionState.Idle)
    val connection: StateFlow<ConnectionState> = _connection.asStateFlow()

    init {
        container.settingsRepository.settings
            .onEach { _settings.value = it }
            .launchIn(viewModelScope)
    }

    fun saveSettings(serverUrl: String, token: String, collection: String) {
        val updated = AxonSettings(serverUrl = serverUrl.trim(), token = token.trim(), collection = collection.trim())
        viewModelScope.launch {
            container.settingsRepository.save(updated)
            container.axonClient.updateConfig(updated.serverUrl, updated.token)
        }
    }

    fun testConnection(serverUrl: String, token: String) {
        viewModelScope.launch {
            _connection.value = ConnectionState.Testing
            container.axonClient.updateConfig(serverUrl.trim(), token.trim())
            val ok = container.axonClient.healthz()
            _connection.value = if (ok) ConnectionState.Ok else ConnectionState.Failed("Server unreachable")
        }
    }
}
```

- [ ] **Step 2: Create `SettingsScreen.kt`**

```kotlin
package com.axon.app.ui.settings

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Error
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraTextField

@Composable
fun SettingsScreen(vm: SettingsViewModel = viewModel()) {
    val settings by vm.settings.collectAsStateWithLifecycle()
    val connection by vm.connection.collectAsStateWithLifecycle()

    var serverUrl  by remember(settings.serverUrl)  { mutableStateOf(settings.serverUrl) }
    var token      by remember(settings.token)      { mutableStateOf(settings.token) }
    var collection by remember(settings.collection) { mutableStateOf(settings.collection) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("Settings", style = MaterialTheme.typography.headlineMedium)

        AuroraTextField(
            value = serverUrl,
            onValueChange = { serverUrl = it },
            label = "Server URL",
            modifier = Modifier.fillMaxWidth(),
        )

        AuroraTextField(
            value = token,
            onValueChange = { token = it },
            label = "API Token",
            modifier = Modifier.fillMaxWidth(),
            visualTransformation = PasswordVisualTransformation(),
        )

        AuroraTextField(
            value = collection,
            onValueChange = { collection = it },
            label = "Collection",
            modifier = Modifier.fillMaxWidth(),
        )

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            AuroraButton(
                text = "Save",
                onClick = { vm.saveSettings(serverUrl, token, collection) },
                modifier = Modifier.weight(1f),
            )
            AuroraButton(
                text = if (connection is ConnectionState.Testing) "Testing…" else "Test",
                onClick = { vm.testConnection(serverUrl, token) },
                variant = AuroraButtonVariant.Outlined,
                modifier = Modifier.weight(1f),
                enabled = connection !is ConnectionState.Testing,
            )
        }

        when (val c = connection) {
            is ConnectionState.Ok ->
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Icon(Icons.Default.CheckCircle, null, tint = MaterialTheme.colorScheme.primary)
                    Text("Connected", color = MaterialTheme.colorScheme.primary)
                }
            is ConnectionState.Failed ->
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Icon(Icons.Default.Error, null, tint = MaterialTheme.colorScheme.error)
                    Text(c.error, color = MaterialTheme.colorScheme.error)
                }
            else -> {}
        }
    }
}
```

---

## Task 8: Ask Screen (Main Feature)

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/ask/AskViewModel.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/ask/AskScreen.kt`

- [ ] **Step 1: Create `AskViewModel.kt`**

```kotlin
package com.axon.app.ui.ask

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.repository.AskResultUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

sealed interface AskUiState {
    object Idle : AskUiState
    object Loading : AskUiState
    data class Success(val result: AskResultUi) : AskUiState
    data class Error(val message: String) : AskUiState
}

class AskViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<AskUiState>(AskUiState.Idle)
    val uiState: StateFlow<AskUiState> = _uiState.asStateFlow()

    val history = container.askHistoryDao.recent()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun ask(query: String) {
        if (query.isBlank()) return
        viewModelScope.launch {
            _uiState.value = AskUiState.Loading
            container.axonRepository.ask(query).fold(
                onSuccess = { result ->
                    _uiState.value = AskUiState.Success(result)
                    container.askHistoryDao.insert(
                        AskHistoryEntry(query = result.query, answer = result.answer)
                    )
                },
                onFailure = { err ->
                    _uiState.value = AskUiState.Error(err.message ?: "Unknown error")
                },
            )
        }
    }

    fun clearState() {
        _uiState.value = AskUiState.Idle
    }
}
```

- [ ] **Step 2: Create `AskScreen.kt`**

```kotlin
package com.axon.app.ui.ask

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.local.AskHistoryEntry
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraPromptInput
import tv.tootie.aurora.components.AuroraThinking
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable
fun AskScreen(vm: AskViewModel = viewModel()) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()
    val history by vm.history.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Ask Axon", style = MaterialTheme.typography.headlineMedium)

        // Current result / loading state
        when (val state = uiState) {
            is AskUiState.Loading -> {
                Box(modifier = Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                    AuroraThinking(label = "Searching knowledge base…")
                }
            }
            is AskUiState.Success -> {
                AuroraCard(
                    modifier = Modifier.fillMaxWidth(),
                    variant = AuroraCardVariant.Filled,
                ) {
                    Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(
                            "Q: ${state.result.query}",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Text(state.result.answer, style = MaterialTheme.typography.bodyMedium)
                        state.result.timingMs?.let { ms ->
                            Text(
                                "${ms}ms",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
            is AskUiState.Error -> {
                AuroraCallout(
                    title = "Error",
                    message = state.message,
                    variant = AuroraCalloutVariant.Error,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            is AskUiState.Idle -> {}
        }

        Spacer(Modifier.weight(1f))

        // History
        AnimatedVisibility(visible = history.isNotEmpty() && uiState is AskUiState.Idle) {
            Column {
                Text(
                    "Recent",
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(4.dp))
                LazyColumn(
                    modifier = Modifier.heightIn(max = 220.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    items(history, key = { it.id }) { entry ->
                        HistoryCard(entry = entry, onClick = { input = entry.query })
                    }
                }
            }
        }

        // Composer
        AuroraPromptInput(
            value = input,
            onValueChange = { input = it },
            onSend = {
                vm.ask(input)
                input = ""
            },
            placeholder = "Ask anything about your indexed knowledge…",
            loading = uiState is AskUiState.Loading,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun HistoryCard(entry: AskHistoryEntry, onClick: () -> Unit) {
    val fmt = remember { SimpleDateFormat("HH:mm", Locale.getDefault()) }
    AuroraCard(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
        variant = AuroraCardVariant.Outlined,
    ) {
        Column(modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp)) {
            Text(entry.query, style = MaterialTheme.typography.bodySmall, maxLines = 1)
            Text(
                fmt.format(Date(entry.askedAt)),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
```

---

## Task 9: Search Screen

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/search/SearchViewModel.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/search/SearchScreen.kt`

- [ ] **Step 1: Create `SearchViewModel.kt`**

```kotlin
package com.axon.app.ui.search

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.QueryHitUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

sealed interface SearchUiState {
    object Idle : SearchUiState
    object Loading : SearchUiState
    data class Results(val hits: List<QueryHitUi>) : SearchUiState
    data class Error(val message: String) : SearchUiState
}

class SearchViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<SearchUiState>(SearchUiState.Idle)
    val uiState: StateFlow<SearchUiState> = _uiState.asStateFlow()

    fun search(query: String) {
        if (query.isBlank()) return
        viewModelScope.launch {
            _uiState.value = SearchUiState.Loading
            container.axonRepository.query(query, limit = 20).fold(
                onSuccess = { hits -> _uiState.value = SearchUiState.Results(hits) },
                onFailure = { err -> _uiState.value = SearchUiState.Error(err.message ?: "Error") },
            )
        }
    }
}
```

- [ ] **Step 2: Create `SearchScreen.kt`**

```kotlin
package com.axon.app.ui.search

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.QueryHitUi
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraPromptInput
import tv.tootie.aurora.components.AuroraThinking

@Composable
fun SearchScreen(vm: SearchViewModel = viewModel()) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Vector Search", style = MaterialTheme.typography.headlineMedium)

        when (val state = uiState) {
            is SearchUiState.Loading -> {
                Box(modifier = Modifier.fillMaxWidth().weight(1f), contentAlignment = Alignment.Center) {
                    AuroraThinking(label = "Searching vectors…")
                }
            }
            is SearchUiState.Results -> {
                if (state.hits.isEmpty()) {
                    Box(modifier = Modifier.weight(1f), contentAlignment = Alignment.Center) {
                        Text("No results", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                } else {
                    LazyColumn(
                        modifier = Modifier.weight(1f),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        itemsIndexed(state.hits, key = { _, h -> h.url + h.rank }) { _, hit ->
                            SearchHitCard(hit)
                        }
                    }
                }
            }
            is SearchUiState.Error -> {
                AuroraCallout(
                    title = "Error",
                    message = state.message,
                    variant = AuroraCalloutVariant.Error,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.weight(1f))
            }
            is SearchUiState.Idle -> Spacer(Modifier.weight(1f))
        }

        AuroraPromptInput(
            value = input,
            onValueChange = { input = it },
            onSend = { vm.search(input) },
            placeholder = "Search indexed knowledge…",
            loading = uiState is SearchUiState.Loading,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun SearchHitCard(hit: QueryHitUi) {
    val uriHandler = LocalUriHandler.current
    AuroraCard(
        onClick = { runCatching { uriHandler.openUri(hit.url) } },
        modifier = Modifier.fillMaxWidth(),
        variant = AuroraCardVariant.Outlined,
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(
                    hit.source,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    "%.3f".format(hit.score),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Text(hit.snippet, style = MaterialTheme.typography.bodySmall, maxLines = 3)
        }
    }
}
```

---

## Task 10: Sources Screen

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/sources/SourcesViewModel.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/sources/SourcesScreen.kt`

- [ ] **Step 1: Create `SourcesViewModel.kt`**

```kotlin
package com.axon.app.ui.sources

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.SourceEntryUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

sealed interface SourcesUiState {
    object Loading : SourcesUiState
    data class Loaded(val sources: List<SourceEntryUi>, val total: Int) : SourcesUiState
    data class Error(val message: String) : SourcesUiState
}

class SourcesViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<SourcesUiState>(SourcesUiState.Loading)
    val uiState: StateFlow<SourcesUiState> = _uiState.asStateFlow()

    init { load() }

    fun load() {
        viewModelScope.launch {
            _uiState.value = SourcesUiState.Loading
            container.axonRepository.sources(limit = 100).fold(
                onSuccess = { list ->
                    _uiState.value = SourcesUiState.Loaded(
                        sources = list,
                        total = list.sumOf { it.chunks },
                    )
                },
                onFailure = { err -> _uiState.value = SourcesUiState.Error(err.message ?: "Error") },
            )
        }
    }
}
```

- [ ] **Step 2: Create `SourcesScreen.kt`**

```kotlin
package com.axon.app.ui.sources

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.SourceEntryUi
import tv.tootie.aurora.components.AuroraBadge
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraItem
import tv.tootie.aurora.components.AuroraProgress

@Composable
fun SourcesScreen(vm: SourcesViewModel = viewModel()) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        Text("Sources", style = MaterialTheme.typography.headlineMedium)
        Spacer(Modifier.height(8.dp))

        when (val state = uiState) {
            is SourcesUiState.Loading -> {
                AuroraProgress(modifier = Modifier.fillMaxWidth())
            }
            is SourcesUiState.Error -> {
                AuroraCallout(
                    title = "Error",
                    message = state.message,
                    variant = AuroraCalloutVariant.Error,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            is SourcesUiState.Loaded -> {
                Text(
                    "${state.sources.size} sources · ${state.total} chunks",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(8.dp))
                LazyColumn(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    items(state.sources, key = { it.url }) { entry ->
                        SourceRow(entry)
                    }
                }
            }
        }
    }
}

@Composable
private fun SourceRow(entry: SourceEntryUi) {
    val uriHandler = LocalUriHandler.current
    AuroraItem(
        headlineContent = {
            Text(
                entry.url,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        },
        trailingContent = {
            AuroraBadge(count = entry.chunks)
        },
        onClick = { runCatching { uriHandler.openUri(entry.url) } },
    )
}
```

---

## Task 11: Wire Settings → Client Updates + Build Verification

**Files:**
- Modify: `apps/android/app/src/main/java/com/axon/app/AxonApp.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/di/AppContainer.kt`

The SettingsViewModel already calls `container.axonClient.updateConfig()` on save. We need `AxonApp.onCreate()` to also apply stored settings at launch so the client has the right URL + token on cold start.

- [ ] **Step 1: Update `AppContainer.kt` to accept startup config**

```kotlin
package com.axon.app.di

import android.content.Context
import com.axon.app.data.local.AppDatabase
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.AxonRepository
import com.axon.app.data.repository.SettingsRepository
import com.axon.app.data.repository.DEFAULT_SERVER_URL

class AppContainer(context: Context) {
    val settingsRepository = SettingsRepository(context)
    private val db = AppDatabase.build(context)
    val askHistoryDao = db.askHistoryDao()

    val axonClient = AxonClient(
        baseUrl = DEFAULT_SERVER_URL,
        token = "",
    )

    val axonRepository = AxonRepository(axonClient)

    // Called once at app start after settings are read from DataStore
    fun applySettings(serverUrl: String, token: String) {
        axonClient.updateConfig(serverUrl, token)
    }
}
```

- [ ] **Step 2: Update `AxonApp.kt` to read and apply settings at start**

```kotlin
package com.axon.app

import android.app.Application
import com.axon.app.di.AppContainer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

class AxonApp : Application() {
    lateinit var container: AppContainer
        private set

    private val appScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
        appScope.launch {
            val s = container.settingsRepository.settings.first()
            container.applySettings(s.serverUrl, s.token)
        }
    }
}
```

- [ ] **Step 3: Full compile + test**

```bash
cd apps/android && ./gradlew :app:assembleDebug 2>&1 | tail -40
```
Expected: BUILD SUCCESSFUL, APK at `app/build/outputs/apk/debug/app-debug.apk`.

- [ ] **Step 4: Run unit tests**

```bash
cd apps/android && ./gradlew :app:testDebugUnitTest 2>&1 | tail -20
```
Expected: All 3 client tests PASS.

- [ ] **Step 5: Install on device**

```bash
cd apps/android && ./gradlew :app:installDebug 2>&1 | tail -20
```
Expected: BUILD SUCCESSFUL (requires connected device or running emulator).

- [ ] **Step 6: Commit**

```bash
git -C /home/jmagar/workspace/axon_rust add apps/android/
git -C /home/jmagar/workspace/axon_rust commit -m "feat(android): Axon Android app with Ask/Search/Sources/Settings screens using Aurora design system"
```

---

## Task 12: Visual Verification with claude-in-mobile

Testing is done via `claude-in-mobile` MCP tools routed through the lab gateway. Run these checks after `installDebug` succeeds.

- [ ] **Step 1: Launch the app**

```
mcp__claude-in-mobile__launch_app  package: com.axon.app
```

- [ ] **Step 2: Screenshot initial state (Ask tab)**

```
mcp__claude-in-mobile__screenshot
```
Expected: Ask screen with AuroraTheme navy background, AuroraPromptInput at bottom, "Ask Axon" heading.

- [ ] **Step 3: Verify settings tab**

```
mcp__claude-in-mobile__tap  element: "Settings"
mcp__claude-in-mobile__screenshot
```
Expected: Settings screen with server URL, token, collection fields pre-populated from DataStore.

- [ ] **Step 4: Enter token and test connection**

```
mcp__claude-in-mobile__tap  element: "API Token"
mcp__claude-in-mobile__input_text  text: "<token from ~/.axon/.env>"
mcp__claude-in-mobile__tap  element: "Test"
mcp__claude-in-mobile__screenshot
```
Expected: "Connected" status with check icon after ~2s.

- [ ] **Step 5: Test Ask flow**

```
mcp__claude-in-mobile__tap  element: "Ask"
mcp__claude-in-mobile__tap  element: "Message input"
mcp__claude-in-mobile__input_text  text: "What is Axon?"
mcp__claude-in-mobile__tap  element: "Send message"
mcp__claude-in-mobile__screenshot
```
Expected: AuroraThinking indicator, then answer card with response text.

- [ ] **Step 6: Capture logs if any screen fails**

```
mcp__claude-in-mobile__get_logs  package: com.axon.app
```

---

## Self-Review Notes

- **SourcesRequest.collection**: Currently not passed to `GET /v1/sources` (GET has no body). The `?collection=` query param can be added to `AxonClient.sources()` if needed — omitted for now as the collection defaults server-side from `AXON_COLLECTION` env.
- **No pagination on Sources**: `SourcesScreen` loads 100 items on init. Add infinite scroll as a follow-up.
- **Search uses `/v1/query`** (vector search), not `/v1/search` (web search via Tavily) — this matches the mobile use case of searching the indexed knowledge base.
- **AuroraCard/AuroraCallout/AuroraItem API**: Verify exact parameter names match the Aurora library version by checking component source in `aurora-design-system/android/aurora/src/main/kotlin/tv/tootie/aurora/components/` if compile errors occur. The Aurora library uses `public` Kotlin API (`explicitApi()` enforced).
