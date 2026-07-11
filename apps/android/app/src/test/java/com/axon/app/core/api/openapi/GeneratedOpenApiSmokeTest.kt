package com.axon.app.core.api.openapi

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

        val discoveryApi = OpenApiTestPaths.generatedKotlinRoot.resolve("com/axon/app/generated/api/DiscoveryApi.kt")
        assertTrue("Generated DiscoveryApi.kt should exist", discoveryApi.isFile)
        assertTrue(
            "Generated DiscoveryApi should expose GET /v1/collections",
            discoveryApi.readText().contains("collectionsOpenapiMarker"),
        )

        val apiClient = OpenApiTestPaths.generatedKotlinRoot.resolve("org/openapitools/client/infrastructure/ApiClient.kt")
        assertTrue("Generated ApiClient.kt should exist", apiClient.isFile)
        val apiClientText = apiClient.readText()
        assertTrue(
            "Generated ApiClient.kt should use Android-compatible temp-file creation",
            apiClientText.contains("java.io.File.createTempFile(prefix, suffix)"),
        )
        assertTrue(
            "Generated ApiClient.kt should not retain java.nio.file temp-file creation",
            !apiClientText.contains("java.nio.file.Files.createTempFile(prefix, suffix).toFile()"),
        )
    }
}
