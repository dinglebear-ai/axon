package com.axon.app.data.remote

import com.axon.app.data.remote.models.MemoryRequestDto
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * Covers the memory client surface (android-contract.md "memory" row):
 * remember, search, context, show-by-id, forget-by-id.
 */
class AxonClientMemoryTest {
    private lateinit var server: MockWebServer
    private lateinit var client: AxonClient

    @Before fun setUp() {
        server = MockWebServer().also { it.start() }
        client = AxonClient(server.url("/").toString().trimEnd('/'), "test-token")
    }
    @After fun tearDown() { server.shutdown() }

    @Test fun `rememberMemory posts to v1 memories`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"id":"m1","status":"ok"}""").addHeader("Content-Type", "application/json"))
        val r = client.rememberMemory(MemoryRequestDto(memoryType = "fact", title = "t", body = "b"))
        assertTrue(r.isSuccess)
        assertEquals("m1", r.getOrThrow().jsonObject["id"]!!.jsonPrimitive.content)
        val req = server.takeRequest()
        assertEquals("POST", req.method)
        assertEquals("/v1/memories", req.path)
        val body = req.body.readUtf8()
        assertTrue(body.contains("\"memory_type\":\"fact\""))
        assertTrue(body.contains("\"title\":\"t\""))
        // Unset fields must not be serialized — the server's RestMemoryRequest
        // uses #[serde(deny_unknown_fields)] but kotlinx omits null defaults anyway.
        assertTrue(!body.contains("\"query\""))
    }

    @Test fun `searchMemories posts to v1 memories search`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"results":[{"id":"m1"}]}""").addHeader("Content-Type", "application/json"))
        val r = client.searchMemories(MemoryRequestDto(query = "auth flow", limit = 5))
        assertTrue(r.isSuccess)
        val req = server.takeRequest()
        assertEquals("POST", req.method)
        assertEquals("/v1/memories/search", req.path)
        val body = req.body.readUtf8()
        assertTrue(body.contains("\"query\":\"auth flow\""))
        assertTrue(body.contains("\"limit\":5"))
    }

    @Test fun `memoryContext posts to v1 memories context`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"context":"..."}""").addHeader("Content-Type", "application/json"))
        val r = client.memoryContext(MemoryRequestDto(project = "axon", repo = "axon"))
        assertTrue(r.isSuccess)
        val req = server.takeRequest()
        assertEquals("POST", req.method)
        assertEquals("/v1/memories/context", req.path)
        val body = req.body.readUtf8()
        assertTrue(body.contains("\"project\":\"axon\""))
        assertTrue(body.contains("\"repo\":\"axon\""))
    }

    @Test fun `getMemory GETs v1 memories memory_id`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"id":"m1","title":"t"}""").addHeader("Content-Type", "application/json"))
        val r = client.getMemory("m1")
        assertTrue(r.isSuccess)
        assertEquals("m1", r.getOrThrow().jsonObject["id"]!!.jsonPrimitive.content)
        val req = server.takeRequest()
        assertEquals("GET", req.method)
        assertEquals("/v1/memories/m1", req.path)
    }

    @Test fun `getMemory URL-encodes the memory id path segment`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"id":"m 1"}""").addHeader("Content-Type", "application/json"))
        client.getMemory("m 1")
        val req = server.takeRequest()
        assertEquals("/v1/memories/m%201", req.path)
    }

    @Test fun `deleteMemory DELETEs v1 memories memory_id`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"id":"m1","status":"forgotten"}""").addHeader("Content-Type", "application/json"))
        val r = client.deleteMemory("m1")
        assertTrue(r.isSuccess)
        assertEquals("forgotten", r.getOrThrow().jsonObject["status"]!!.jsonPrimitive.content)
        val req = server.takeRequest()
        assertEquals("DELETE", req.method)
        assertEquals("/v1/memories/m1", req.path)
    }
}
