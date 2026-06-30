# Axon Android

> Current pre-#298 Android client docs. The future Android/mobile contract is
> `docs/pipeline-unification/surfaces/android-contract.md`; after the
> source-pipeline cutover, Android route fixtures should assert the shared
> source/job/memory/graph REST contract.

Native Android client for Axon. The app talks to the Axon HTTP API, uses the
Aurora Android composite when available, and keeps mobile-specific safety checks
close to the UI paths that submit work.

## Build And Verification

Use the wrapper in this directory from the repository root. Dependency
verification is enabled for these commands through
`gradle/verification-metadata.xml`; do not disable it when validating Android
changes.

```bash
apps/android/gradlew -p apps/android :app:compileDebugKotlin :app:testDebugUnitTest :app:lintDebug --no-daemon
apps/android/gradlew -p apps/android :app:compileDebugAndroidTestKotlin --no-daemon
apps/android/gradlew -p apps/android :app:assembleRelease --no-daemon
```

AGP 8.13.2 requires a Gradle wrapper on 8.13 or newer. If these commands fail
before dependency resolution with `Minimum supported Gradle version is 8.13`,
update `gradle/wrapper/gradle-wrapper.properties` first, then rerun the full
command set above.

Refresh it only when Android dependencies intentionally change:

```bash
apps/android/gradlew -p apps/android --write-verification-metadata sha256 <tasks> --no-daemon
```

## Aurora Composite And Lint

`settings.gradle.kts` auto-detects a local Aurora Android checkout unless
`axonAuroraAndroidPath` or `AXON_AURORA_ANDROID_PATH` points elsewhere.

The app is on AGP 8.13.2. Keep the optional Aurora composite on a compatible
AGP line when it is included; Gradle rejects incompatible Android Gradle Plugin
versions in one composite build.

`android.suppressUnsupportedCompileSdk=36` remains in `gradle.properties`
because the optional Aurora composite may compile SDK 36.

`app/lint.xml` ignores only `NullSafeMutableLiveData`. Keep the suppression only
while `lintDebug` still hits the lifecycle detector/Kotlin Analysis API crash;
remove it after the detector runs normally with the current AGP/Kotlin pair.

## Connection Security

The app rejects arbitrary `http://` server URLs. Cleartext is accepted only for
the Tailscale domains declared in `app/src/main/res/xml/network_security_config.xml`:

- `manatee-triceratops.ts.net`
- `manatee-triceratops.tailvpn.net`

Use `https://` for any other host.

Panel endpoints require an explicit panel unlock in the app. `/api/panel/*`
requests are blocked locally until a non-blank panel token has been stored from
`/api/panel/login`; the normal API bearer token is not reused for panel routes.
Settings file values are redacted before they enter UI state. Private raw file
text is retained only inside `SettingsViewModel` for patching unchanged saves.
Dirty redacted placeholders are filtered out before saves, so an unchanged
masked secret is not written back over the real server value.

## FAB And Jobs Behavior

FAB ingest source inference uses the same host-aware `IngestSource` validation
as the dedicated ingest screen. Canonical hosts and real subdomains are accepted;
lookalike hosts such as `github.com.attacker.com` are rejected before submission.
Non-URL shorthand such as `github/owner/repo` and `r/subreddit` remains supported.

Async FAB result cards poll job status for a bounded window after submission and
then defer to Jobs for ongoing live status. The Jobs overview poller starts only
while the drawer or Jobs screen is visible and stops when those composables leave
the UI.

## Document Parser Fixtures

The custom retrieve-output parser is fixture-driven. See
`docs/document-parser-fixtures.md` before changing `DocumentParsing.kt`.

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
