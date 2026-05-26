package com.axon.app.data.remote

import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class AxonClientTest {

    private lateinit var server: MockWebServer
    private lateinit var client: AxonClient

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
        client = AxonClient(
            baseUrl = server.url("/").toString().trimEnd('/'),
            token = "test-token",
        )
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    @Test
    fun `healthz returns true when server responds 200`() = runBlocking {
        server.enqueue(MockResponse().setBody("ok").setResponseCode(200))
        val healthy = client.healthz()
        assertTrue(healthy)
        val req = server.takeRequest()
        assertEquals("/healthz", req.path)
    }

    @Test
    fun `ask sends auth header and deserializes response`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"query":"hello","answer":"world","timing_ms":{"total_ms":500}}""")
                .addHeader("Content-Type", "application/json")
        )
        val result = client.ask(AskRequest(query = "hello"))
        assertTrue(result.isSuccess)
        assertEquals("world", result.getOrThrow().answer)
        val req = server.takeRequest()
        assertEquals("Bearer test-token", req.getHeader("Authorization"))
        assertEquals("/v1/ask", req.path)
    }

    @Test
    fun `query deserializes results list`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"results":[{"rank":1,"score":0.9,"rerank_score":0.0,"url":"https://a.com","source":"a.com","snippet":"some text","chunk_index":null}]}""")
                .addHeader("Content-Type", "application/json")
        )
        val result = client.query(QueryRequest(query = "test"))
        assertTrue(result.isSuccess)
        assertEquals(1, result.getOrThrow().results.size)
        assertEquals("https://a.com", result.getOrThrow().results[0].url)
    }
}
