// Top-level build file — no source here. Plugin versions come from the version catalog.
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.android)       apply false
    alias(libs.plugins.compose.compiler)     apply false
    alias(libs.plugins.kotlinx.serialization) apply false
    alias(libs.plugins.ksp)                  apply false
}
