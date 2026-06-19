package com.axon.app.data.remote.openapi

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

class AndroidOpenApiRouteContractTest {
    private val json = Json { ignoreUnknownKeys = true }

    private val requiredAndroidRoutes = listOf(
        Route("POST", "/v1/ask", true),
        Route("POST", "/v1/chat", true),
        Route("POST", "/v1/ask/stream", true),
        Route("POST", "/v1/chat/stream", true),
        Route("POST", "/v1/query", true),
        Route("POST", "/v1/retrieve", true),
        Route("GET", "/v1/sources", true),
        Route("GET", "/v1/stats", true),
        Route("POST", "/v1/scrape", true),
        Route("POST", "/v1/map", true),
        Route("POST", "/v1/research", true),
        Route("POST", "/v1/crawl", true),
        Route("GET", "/v1/crawl/{id}", true),
        Route("POST", "/v1/summarize", true),
        Route("POST", "/v1/search", true),
        Route("POST", "/v1/ingest", true),
        Route("POST", "/v1/extract", true),
        Route("POST", "/v1/embed", true),
        Route("GET", "/v1/crawl", true),
        Route("GET", "/v1/embed", true),
        Route("GET", "/v1/extract", true),
        Route("GET", "/v1/ingest", true),
        Route("GET", "/v1/embed/{id}", true),
        Route("GET", "/v1/extract/{id}", true),
        Route("GET", "/v1/ingest/{id}", true),
        Route("POST", "/v1/crawl/{id}/cancel", true),
        Route("POST", "/v1/embed/{id}/cancel", true),
        Route("POST", "/v1/extract/{id}/cancel", true),
        Route("POST", "/v1/ingest/{id}/cancel", true),
        Route("GET", "/v1/status", true),
        Route("GET", "/v1/doctor", true),
        Route("POST", "/v1/suggest", true),
        Route("GET", "/v1/domains", true),
        Route("GET", "/v1/watch", true),
        Route("GET", "/v1/mobile/sessions", true),
        Route("GET", "/v1/mobile/sessions/{id}", true),
        Route("PUT", "/v1/mobile/sessions/{id}", true),
        Route("DELETE", "/v1/mobile/sessions/{id}", true),
        Route("GET", "/v1/artifacts", true),
        Route("GET", "/v1/collections", true),
    )

    private val forbiddenGeneratedRoutes = listOf(
        "/api/panel/config",
        "/api/panel/env",
        "/api/panel/collections",
        "/api/panel/artifact",
        "/api/panel/command",
    )

    @Test
    fun androidRoutesExistInOpenApiWithExpectedSecurity() {
        val paths = pathsObject()
        val failures = requiredAndroidRoutes.flatMap { route ->
            val operation = paths[route.path]?.jsonObject?.get(route.method.lowercase())?.jsonObject
            when {
                operation == null -> listOf("${route.method} ${route.path} missing")
                route.requiresAuth && operation["security"] == null -> listOf("${route.method} ${route.path} missing security")
                else -> emptyList()
            }
        }

        assertEquals(emptyList<String>(), failures)
    }

    @Test
    fun panelConfigRoutesAreNotInGeneratedOpenApiSurface() {
        val paths = pathsObject().keys
        val exposed = forbiddenGeneratedRoutes.filter { it in paths }

        assertEquals(emptyList<String>(), exposed)
    }

    @Test
    fun healthRoutesAreTheOnlyPublicNonDocsRuntimeRoutesInThisContract() {
        val paths = pathsObject()
        val publicRuntimeRoutes = paths.entries
            .filter { (path, _) -> path == "/healthz" || path == "/readyz" }
            .filter { (_, item) -> item.jsonObject.values.any { operation -> operation.jsonObject["security"] == null } }
            .map { it.key }
            .sorted()

        assertEquals(listOf("/healthz", "/readyz"), publicRuntimeRoutes)
    }

    private fun pathsObject() = json.parseToJsonElement(OpenApiTestPaths.openApiJson.readText())
        .jsonObject
        .getValue("paths")
        .jsonObject

    private data class Route(
        val method: String,
        val path: String,
        val requiresAuth: Boolean,
    )
}
