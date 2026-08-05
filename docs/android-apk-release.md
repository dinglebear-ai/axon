---
title: "Android APK Release Workflow"
created: 2026-06-09
updated: 2026-08-03
---

# Android APK Release Workflow

Android is a release-please-driven component. Release-please is the sole
normal owner of its version edits, changelog, `android-v*` tag, and GitHub
Release. Axon's Android workflow builds and attaches the APK; it does not
create a competing release record.

## Ownership and sequence

1. An ordinary Android feature/fix PR changes `apps/android` shipping files
   but leaves `versionName`, `versionCode`, the Android changelog, and
   `.release-please-manifest.json` alone.
2. `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`
   validates current parity and defers the managed version bump.
3. After green main CI, release-please opens or refreshes the Android release
   PR. That PR owns the manifest, changelog, `versionName`, and `versionCode`.
4. Merging the green release PR lets release-please create the `android-v*`
   tag and GitHub Release.
5. `.github/workflows/release-please.yml` dispatches
   `.github/workflows/android-release.yml` at that exact tag with
   `publish=true`; the workflow signs, checksums, and uploads the APK to the
   existing Release.

`.github/workflows/auto-tag.yml` must never select Android. Auto-tag is only
for components whose `release_driver = "axon-native"` (currently the CLI).

## Build behavior

| Aspect | Behavior |
|---|---|
| Version guard | An `android-v*` dispatch must match `versionName` in `apps/android/app/build.gradle.kts`. |
| Build | Checks out Aurora, installs its token dependencies, configures JDK/Gradle/Android SDK, and assembles the release APK. |
| Sign | Uses zipalign and apksigner when all four signing secrets exist. A publish request fails closed if they are missing. |
| Publish | Uploads APK and SHA256 as a run artifact, then attaches both to the pre-existing release-please GitHub Release. |
| Dry run | Manual `workflow_dispatch` with `publish=false` builds and uploads a run artifact without changing Releases. |

## Required configuration

Set these under repository Actions secrets and variables.

Signing secrets (all four are required to publish):

- `ANDROID_KEYSTORE_BASE64`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`

Aurora configuration:

- `AURORA_REPO` repository variable (defaults to the configured Aurora source)
- optional `AURORA_REF`
- optional `AURORA_TOKEN` for a private checkout

## Verification

Before merging an Android release PR:

```bash
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
```

Then require green Android CI and confirm the generated release PR advances
the manifest, changelog, `versionName`, and `versionCode` together. After the
release PR merges, confirm the artifact workflow ran at the release-please tag
and attached a signed APK plus checksum to the existing GitHub Release.

Directly creating or pushing an Android tag is a break-glass incident action,
not a normal hotfix shortcut. Never create a second Android version/tag/Release
lane alongside release-please.
