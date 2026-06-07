package com.axon.app.ui.common

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HumanJsonTest {
    @Test
    fun `humanizeJsonText converts object payloads to readable rows`() {
        val rendered = humanizeJsonText(
            """{"error":"LLM answer generation failed","request_id":"abc123","ok":false}""",
        )

        assertTrue(rendered.contains("Error: LLM answer generation failed"))
        assertTrue(rendered.contains("Request Id: abc123"))
        assertTrue(rendered.contains("Ok: No"))
        assertFalse(rendered.contains("{"))
        assertFalse(rendered.contains("\""))
    }

    @Test
    fun `humanizeJsonText leaves normal prose unchanged`() {
        val text = "LLM answer generation failed"

        assertTrue(humanizeJsonText(text) == text)
    }

    @Test
    fun `humanizeJsonFragmentText converts prefixed command payloads`() {
        val rendered = humanizeJsonFragmentText(
            """doctor · {"qdrant":"ok","tei":"ok","vectors":8634279}""",
        )

        assertTrue(rendered.startsWith("doctor ·"))
        assertTrue(rendered.contains("Qdrant: ok"))
        assertTrue(rendered.contains("Vectors: 8,634,279"))
        assertFalse(rendered.contains("{"))
        assertFalse(rendered.contains("\""))
    }

    @Test
    fun `humanizeJsonFragmentText converts embedded error payloads with suffix text`() {
        val rendered = humanizeJsonFragmentText(
            """HTTP 502: {"error":"LLM answer generation failed","retryable":true} from upstream""",
        )

        assertTrue(rendered.contains("HTTP 502:"))
        assertTrue(rendered.contains("Error: LLM answer generation failed"))
        assertTrue(rendered.contains("Retryable: Yes"))
        assertTrue(rendered.contains("from upstream"))
        assertFalse(rendered.contains("{"))
        assertFalse(rendered.contains("\""))
    }

    @Test
    fun `doctorServiceSummary formats nested services as readable stack rows`() {
        val rendered = kotlinx.serialization.json.Json.parseToJsonElement(
            """
            {
              "all_ok": true,
              "services": {
                "qdrant": {
                  "ok": true,
                  "effective_url": "http://127.0.0.1:53333",
                  "collection": "axon",
                  "vector_mode": "named"
                },
                "tei": {
                  "ok": true,
                  "effective_url": "http://127.0.0.1:52000",
                  "model": "Qwen3-Embedding-0.6B"
                },
                "chrome": {
                  "ok": false,
                  "url": "http://axon-chrome:6000",
                  "detail": "http 502"
                }
              }
            }
            """.trimIndent(),
        ).doctorServiceSummary()

        assertTrue(rendered.contains("axon-qdrant · up · 127.0.0.1:53333 · named"))
        assertTrue(rendered.contains("axon-tei · up · 127.0.0.1:52000 · Qwen3-Embedding-0.6B"))
        assertTrue(rendered.contains("axon-chrome · down · axon-chrome:6000 · HTTP 502"))
        assertFalse(rendered.contains("fields"))
        assertFalse(rendered.contains("{"))
        assertFalse(rendered.contains("\""))
    }
}
