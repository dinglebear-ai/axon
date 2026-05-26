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
// Path from worktree apps/android/ → up 5 levels → aurora-design-system/android
// worktree: axon_rust/.worktrees/axon-android-app/apps/android
// target:   workspace/aurora-design-system/android
includeBuild("../../../../../aurora-design-system/android") {
    dependencySubstitution {
        substitute(module("tv.tootie.aurora:aurora")).using(project(":aurora"))
    }
}

rootProject.name = "axon-android"
include(":app")
