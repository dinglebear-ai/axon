plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.kotlinx.serialization)
    alias(libs.plugins.ksp)
}

android {
    namespace = "com.axon.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.axon.app"
        minSdk = 24
        targetSdk = 35
        versionCode = 7
        versionName = "1.3.3"
        manifestPlaceholders["appAuthRedirectScheme"] = "com.axon.app"
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    lint {
        lintConfig = file("lint.xml")
    }

    testOptions {
        unitTests.isIncludeAndroidResources = true
        // Stub Android framework methods (android.util.Log, etc.) so unit tests
        // running on a plain JVM don't throw "method not mocked" RuntimeExceptions.
        // Robolectric-annotated tests still get the full framework via
        // isIncludeAndroidResources above.
        unitTests.isReturnDefaultValues = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

ksp {
    // Room schema export — enables migration verification and schema diffing
    arg("room.schemaLocation", "$projectDir/schemas")
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

    // Security
    implementation(libs.security.crypto)
    implementation(libs.appauth)

    // Tests
    testImplementation(libs.junit)
    testImplementation(libs.mockwebserver)
    testImplementation(libs.turbine)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.robolectric)
    testImplementation(libs.androidx.test.ext.junit)
    // Robolectric-backed Compose composition for unit tests (runComposeUiTest).
    // Lets theme unit tests enter AuroraTheme and read the lib's live ColorScheme /
    // LocalAuroraColors without an instrumented device.
    testImplementation(bom)
    testImplementation(libs.compose.ui.test.junit4)
    testImplementation(libs.compose.ui.test.manifest)

    androidTestImplementation(bom)
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.espresso.core)
    androidTestImplementation(libs.mockwebserver)
    debugImplementation(libs.compose.ui.test.manifest)
}

val repoBinDir = rootProject.layout.projectDirectory.dir("../../bin")

fun registerApkArtifactCopy(variant: String) {
    val capitalized = variant.replaceFirstChar { it.uppercaseChar() }
    val copyTask = tasks.register("copy${capitalized}ApkToRepoBin") {
        dependsOn("assemble$capitalized")
        doLast {
            val apkDir = layout.buildDirectory.dir("outputs/apk/$variant").get().asFile
            val apks = apkDir
                .listFiles { file -> file.isFile && file.extension == "apk" }
                ?.toList()
                .orEmpty()
            require(apks.size == 1) {
                "Expected exactly one $variant APK in ${apkDir.absolutePath}, found ${apks.size}: ${apks.joinToString { it.name }}"
            }
            val dest = repoBinDir.file("axon-android-$variant.apk").asFile
            dest.parentFile.mkdirs()
            apks.single().copyTo(dest, overwrite = true)
            require(dest.isFile && dest.length() > 0) {
                "Copied $variant APK to ${dest.absolutePath}, but the file is missing or empty"
            }
        }
    }
    tasks.matching { it.name == "assemble$capitalized" }.configureEach {
        finalizedBy(copyTask)
    }
}

registerApkArtifactCopy("debug")
registerApkArtifactCopy("release")
