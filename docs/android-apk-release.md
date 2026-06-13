# Android APK Release Workflow

Summary of the Android APK release workflow added in PR
[#195](https://github.com/jmagar/axon/pull/195)
(`feat: add Android APK release workflow`, branch
`claude/apk-release-workflow-ibxnpz`).

## What it is

`.github/workflows/android-release.yml` builds, signs, checksums, and publishes
the Axon Android APK as a GitHub Release. It is modeled on the existing
`chrome-extension-release` workflow so the Android app versions **independently**
of the main axon `v*` releases.

## How it works

| Aspect | Behavior |
|--------|----------|
| **Trigger** | Push a tag `android-v<versionName>` (e.g. `android-v1.1`). |
| **Version guard** | Validates the pushed tag matches `versionName` in `apps/android/app/build.gradle.kts` before building — a release can never ship an APK the tag disagrees with. |
| **Build** | JDK 17 (temurin) + Gradle + Android SDK, then `apps/android/gradlew -p apps/android :app:assembleRelease`. |
| **Sign** | zipalign + `apksigner` using release-keystore secrets, then verifies. Falls back to a clearly named `*-unsigned.apk` when secrets are absent. |
| **Publish** | Uploads APK + SHA256 as a run artifact and, on real tag pushes, creates a GitHub Release with `make_latest: false` (keeps the repo's latest-release badge tracking axon `v*` tags). |
| **Dry-run** | `workflow_dispatch` runs the same build and uploads the artifact **without** creating a Release. |

## Required configuration

Set under **Settings → Secrets and variables → Actions**.

### Signing secrets (all four → signed APK; otherwise unsigned)

- `ANDROID_KEYSTORE_BASE64` — `base64 -w0 release.jks`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`

### Aurora design system (optional)

The app depends on `tv.tootie.aurora:aurora` via a local Gradle composite build
(see `apps/android/settings.gradle.kts`), **not** from Maven. To build it in CI:

- repository **variable** `AURORA_REPO` — the Aurora repo (`owner/name`); checked
  out and wired via `AXON_AURORA_ANDROID_PATH`
- optional variable `AURORA_REF` — pinned branch/tag
- optional secret `AURORA_TOKEN` — for a private Aurora repo

If `AURORA_REPO` is unset, the build proceeds and Aurora resolves from Maven
(which only works if it is actually published there).

## Cutting a release

1. Merge PR #195.
2. Add the signing secrets (and `AURORA_REPO` if Aurora is needed).
3. **Run a `workflow_dispatch` dry-run first** — the Android app has never built
   in CI, so this is the real smoke test for Aurora composite resolution and
   Android SDK build-tools availability on `ubuntu-latest`.
4. Bump `versionName` (and `versionCode`) in `apps/android/app/build.gradle.kts`.
5. Tag and push:

   ```bash
   git tag android-v<versionName>
   git push origin android-v<versionName>
   ```

## Notes

- Mirrors `chrome-extension-release.yml`: component-specific tag, version
  validation, checksum, `make_latest: false`, curated release body.
- The version bump that ships with the PR moves the repo to **5.6.0** across all
  version-bearing files: `Cargo.toml`, `Cargo.lock`, `README.md`, `CHANGELOG.md`,
  `plugins/axon/.claude-plugin/plugin.json`, `apps/web/package.json`,
  `apps/web/package-lock.json`, and the generated `apps/web/openapi/axon.json`
  (the last three are enforced by the `version_bearing_files_stay_in_sync` test
  and the `rest-api-parity` OpenAPI check).
- **Caveat:** the workflow YAML is validated and all PR CI checks are green, but
  the Android build itself has never run in CI. The `workflow_dispatch` dry-run
  is where Aurora resolution and build-tools availability get verified
  end-to-end.
