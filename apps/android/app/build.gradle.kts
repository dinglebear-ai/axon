plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.kotlinx.serialization)
    alias(libs.plugins.ksp)
    alias(libs.plugins.openapi.generator)
}

val axonOpenApiSpec = rootProject.layout.projectDirectory.file("../../apps/web/openapi/axon.json")
val axonOpenApiOutput = layout.buildDirectory.dir("generated/openapi")
val androidTempFilePatchPattern =
    Regex("""java\.nio\.file\.Files\.createTempFile\(([^)]*)\)\.toFile\(\)""")
val androidTempFileReplacement = "java.io.File.createTempFile($1)"

android {
    namespace = "com.axon.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.axon.app"
        minSdk = 24
        targetSdk = 35
        versionCode = 8
        versionName = "1.3.4"
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
        isCoreLibraryDesugaringEnabled = true
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    sourceSets {
        getByName("main") {
            java.srcDir(layout.buildDirectory.dir("generated/openapi/src/main/kotlin"))
        }
    }
}

openApiGenerate {
    generatorName.set("kotlin")
    library.set("jvm-okhttp4")
    inputSpec.set(axonOpenApiSpec.asFile.absolutePath)
    outputDir.set(axonOpenApiOutput.get().asFile.absolutePath)
    apiPackage.set("com.axon.app.generated.api")
    modelPackage.set("com.axon.app.generated.model")
    invokerPackage.set("com.axon.app.generated.invoker")
    validateSpec.set(true)
    configOptions.set(
        mapOf(
            "dateLibrary" to "java8",
            "enumPropertyNaming" to "original",
            "nonPublicApi" to "true",
        )
    )
    globalProperties.set(
        mapOf(
            "apiDocs" to "false",
            "apiTests" to "false",
            "modelDocs" to "false",
            "modelTests" to "false",
        )
    )
}

tasks.named("openApiGenerate") {
    inputs.file(axonOpenApiSpec)
    outputs.dir(axonOpenApiOutput)
    doFirst {
        require(axonOpenApiSpec.asFile.isFile) {
            "Missing OpenAPI spec at ${axonOpenApiSpec.asFile.absolutePath}; run `cargo xtask check-openapi-drift` from repo root."
        }
    }
    doLast {
        val apiClient = axonOpenApiOutput.get()
            .file("src/main/kotlin/org/openapitools/client/infrastructure/ApiClient.kt")
            .asFile
        if (apiClient.isFile) {
            val before = apiClient.readText()
            val matches = androidTempFilePatchPattern.findAll(before).toList()
            require(matches.size == 1) {
                "Generated ApiClient.kt should contain exactly one Android-incompatible java.nio temp-file call; found ${matches.size}."
            }
            val after = before.replace(androidTempFilePatchPattern, androidTempFileReplacement)
            require(!androidTempFilePatchPattern.containsMatchIn(after) && after.contains("java.io.File.createTempFile(")) {
                "Generated ApiClient.kt temp-file patch did not apply cleanly."
            }
            apiClient.writeText(after)
        } else {
            error("Generated ApiClient.kt not found at ${apiClient.absolutePath}; OpenAPI generator layout changed.")
        }
    }
}

tasks.register("verifyOpenApiGeneratedClient") {
    dependsOn("openApiGenerate")
    dependsOn("testDebugUnitTest")
}

tasks.matching { it.name == "compileDebugKotlin" || it.name == "compileReleaseKotlin" }.configureEach {
    dependsOn("openApiGenerate")
}

tasks.matching { it.name == "kspDebugKotlin" || it.name == "kspReleaseKotlin" }.configureEach {
    dependsOn("openApiGenerate")
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
    implementation(libs.moshi)
    implementation(libs.moshi.kotlin)
    coreLibraryDesugaring(libs.desugar.jdk.libs)

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
