package com.axon.app.data.remote.openapi

import java.io.File

internal object OpenApiTestPaths {
    private val workingDir: File = File(System.getProperty("user.dir") ?: ".").canonicalFile
    val androidRoot: File = generateSequence(workingDir) { it.parentFile }
        .first { File(it, "settings.gradle.kts").isFile && File(it, "gradle/libs.versions.toml").isFile }
    val repoRoot: File = requireNotNull(requireNotNull(androidRoot.parentFile).parentFile).canonicalFile
    val openApiJson: File = File(repoRoot, "apps/web/openapi/axon.json")
    val generatedRoot: File = File(androidRoot, "app/build/generated/openapi")
    val generatedKotlinRoot: File = File(generatedRoot, "src/main/kotlin")
}
