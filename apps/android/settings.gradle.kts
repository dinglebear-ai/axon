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

// Optional local-only workaround for older Aurora composite checkouts that hit
// a lifecycle lint / Kotlin Analysis API crash. Keep disabled by default; pass
// -PaxonSkipAuroraLintAnalysis=true only when the Aurora build itself is broken.
val skipAuroraLintAnalysis = providers.gradleProperty("axonSkipAuroraLintAnalysis")
    .map { it.toBoolean() }
    .getOrElse(false)
if (skipAuroraLintAnalysis && gradle.startParameter.taskNames.any { it == ":app:lintDebug" || it == "lintDebug" }) {
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
