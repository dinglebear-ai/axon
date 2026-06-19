package com.axon.app.data.remote.openapi

import org.junit.Assert.assertTrue
import org.junit.Test

class GeneratedOpenApiSmokeTest {
    @Test
    fun generatedKotlinSourcesExistAfterExplicitGeneration() {
        assertTrue(
            "OpenAPI spec should exist at ${OpenApiTestPaths.openApiJson}",
            OpenApiTestPaths.openApiJson.isFile,
        )
        assertTrue(
            "Run `./gradlew :app:openApiGenerate` before this test; missing ${OpenApiTestPaths.generatedKotlinRoot}",
            OpenApiTestPaths.generatedKotlinRoot.isDirectory,
        )
        assertTrue(
            "Generated output should contain Kotlin files",
            OpenApiTestPaths.generatedKotlinRoot.walkTopDown().any { it.isFile && it.extension == "kt" },
        )
    }
}
