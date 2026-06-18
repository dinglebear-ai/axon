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
    fun `humanizeJsonText labels top level arrays`() {
        val rendered = humanizeJsonText("""["alpha"]""")

        assertTrue(rendered.contains("Items: 1 items"))
        assertFalse(rendered.contains("\n: 1 items"))
        assertFalse(rendered.startsWith(":"))
    }

    @Test
    fun `humanizeJsonFragmentText leaves citation markers alone`() {
        val rendered = humanizeJsonFragmentText(
            """
            The dense vector comes from TEI [S6].
            It is stored in Qdrant.
            """.trimIndent(),
        )

        assertTrue(rendered.contains("The dense vector comes from TEI [S6]."))
        assertTrue(rendered.contains("It is stored in Qdrant."))
        assertTrue(rendered.contains("[S6]"))
        assertFalse(rendered.contains("items"))
        assertFalse(rendered.contains("Item 1"))
    }

    @Test
    fun `humanizeJsonText leaves citation marker unchanged`() {
        val rendered = humanizeJsonText("[S6]")

        assertTrue(rendered == "[S6]")
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
