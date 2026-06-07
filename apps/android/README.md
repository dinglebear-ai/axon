# Axon Android

Native Android client for Axon. The app talks to the Axon HTTP API, uses the
Aurora Android composite when available, and keeps mobile-specific safety checks
close to the UI paths that submit work.

## Build And Verification

Use the wrapper in this directory from the repository root:

```bash
apps/android/gradlew -p apps/android :app:compileDebugKotlin :app:testDebugUnitTest :app:lintDebug --no-daemon
apps/android/gradlew -p apps/android :app:compileDebugAndroidTestKotlin --no-daemon
apps/android/gradlew -p apps/android :app:assembleRelease --no-daemon
```

Dependency verification is enabled by `gradle/verification-metadata.xml`.
Refresh it only when Android dependencies intentionally change:

```bash
apps/android/gradlew -p apps/android --write-verification-metadata sha256 <tasks> --no-daemon
```

## Aurora Composite And Lint

`settings.gradle.kts` auto-detects a local Aurora Android checkout unless
`axonAuroraAndroidPath` or `AXON_AURORA_ANDROID_PATH` points elsewhere.

The app and the optional Aurora composite currently stay on AGP 8.7.0 together.
The Aurora composite may compile SDK 36, so `android.suppressUnsupportedCompileSdk=36`
is still present until both builds can move to AGP 8.9.1 or newer in lockstep.
Attempting to move only Axon to AGP 8.9.1 fails because Gradle rejects mixed AGP
versions in one composite build.

`app/lint.xml` ignores only `NullSafeMutableLiveData`. AGP 8.7.0 with Kotlin
2.1.0 crashes inside that lifecycle detector before a lint baseline can apply.
Remove the suppression after the AGP/Kotlin pair is upgraded and `lintDebug`
runs without the Kotlin Analysis API warnings.

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
