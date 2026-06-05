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

// Work around a lifecycle lint / Kotlin analysis API crash in the optional local
// Aurora composite build while preserving app lint. The app receives Aurora's
// compiled AAR metadata; only Aurora's own debug lint analysis task is skipped
// when the requested task is app lint.
if (gradle.startParameter.taskNames.any { it == ":app:lintDebug" || it == "lintDebug" }) {
    gradle.startParameter.excludedTaskNames.add(":android:aurora:lintAnalyzeDebug")
}

// Pull in aurora library from a local composite build when available.
//
// Preferred configuration:
//   ./gradlew -PaxonAuroraAndroidPath=/path/to/aurora-design-system/android ...
// or:
//   AXON_AURORA_ANDROID_PATH=/path/to/aurora-design-system/android ./gradlew ...
//
// If neither is set, probe the standard sibling checkout/worktree layouts.
val configuredAuroraPath = providers.gradleProperty("axonAuroraAndroidPath")
    .orElse(providers.environmentVariable("AXON_AURORA_ANDROID_PATH"))
    .orNull
val auroraRelPaths = listOfNotNull(
    configuredAuroraPath,
    "../../../aurora-design-system/android",
    "../../../../../aurora-design-system/android",
)
val auroraDir = auroraRelPaths.map { file(it) }.firstOrNull { it.isDirectory }
if (auroraDir != null) {
    includeBuild(auroraDir) {
        dependencySubstitution {
            substitute(module("tv.tootie.aurora:aurora")).using(project(":aurora"))
        }
    }
} else {
    logger.warn("Aurora design system not found; set -PaxonAuroraAndroidPath or AXON_AURORA_ANDROID_PATH, otherwise tv.tootie.aurora:aurora will resolve from Maven")
}

rootProject.name = "axon-android"
include(":app")
