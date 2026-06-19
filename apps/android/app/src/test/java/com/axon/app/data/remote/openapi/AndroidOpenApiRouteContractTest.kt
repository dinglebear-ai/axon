package com.axon.app.data.remote.openapi

import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

class AndroidOpenApiRouteContractTest {
    private val json = Json { ignoreUnknownKeys = true }
    private val routeContracts: List<Route> = json.decodeFromString(
        File("src/test/resources/openapi/android-route-contracts.json").readText(),
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
        val failures = routeContracts.flatMap { route ->
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
        val allowedPublicRoutes = routeContracts
            .filterNot { it.requiresAuth }
            .map { "${it.method} ${it.path}" }
            .toSet()
        val publicRuntimeRoutes = paths.entries
            .flatMap { (path, item) ->
                item.jsonObject.entries
                    .filter { (method, _) -> method in openApiMethods }
                    .filter { (_, operation) -> operation.jsonObject["security"] == null }
                    .map { (method, _) -> "${method.uppercase()} $path" }
            }
            .filterNot { it in allowedPublicRoutes }
            .sorted()

        assertEquals(emptyList<String>(), publicRuntimeRoutes)
    }

    private fun pathsObject() = json.parseToJsonElement(OpenApiTestPaths.openApiJson.readText())
        .jsonObject
        .getValue("paths")
        .jsonObject

    @Serializable
    private data class Route(
        val method: String,
        val path: String,
        val requiresAuth: Boolean,
    )

    private companion object {
        val openApiMethods = setOf("get", "put", "post", "delete", "options", "head", "patch", "trace")
    }
}
