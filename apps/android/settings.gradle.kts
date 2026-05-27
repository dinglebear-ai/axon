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
// Supports both the main checkout (axon_rust/apps/android) and worktree
// (.worktrees/<slug>/apps/android) locations by probing both candidate paths.
val auroraRelPaths = listOf(
    "../../../aurora-design-system/android",        // main checkout: axon_rust/apps/android
    "../../../../../aurora-design-system/android",  // worktree: .worktrees/<slug>/apps/android
)
val auroraDir = auroraRelPaths.map { file(it) }.firstOrNull { it.isDirectory }
if (auroraDir != null) {
    includeBuild(auroraDir) {
        dependencySubstitution {
            substitute(module("tv.tootie.aurora:aurora")).using(project(":aurora"))
        }
    }
} else {
    logger.warn("Aurora design system not found at expected paths; tv.tootie.aurora:aurora will resolve from Maven")
}

rootProject.name = "axon-android"
include(":app")
