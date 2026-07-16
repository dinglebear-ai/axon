package com.axon.app.feature.settings

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class SettingsFileTextTest {
    @Test fun `patchEnvText preserves unknown keys and patches only dirty values`() {
        val raw = """
            # production only
            CUSTOM_SECRET=keep-me
            AXON_COLLECTION=old
            CUSTOM_MODEL=unchanged
        """.trimIndent() + "\n"

        val patched = patchEnvText(
            raw = raw,
            values = mapOf(
                "AXON_COLLECTION" to "new collection",
                "CUSTOM_MODEL" to "should-not-write",
            ),
            dirtyKeys = setOf("AXON_COLLECTION"),
        )

        assertTrue(patched.contains("CUSTOM_SECRET=keep-me"))
        assertTrue(patched.contains("AXON_COLLECTION=\"new collection\""))
        assertTrue(patched.contains("CUSTOM_MODEL=unchanged"))
    }

    @Test fun `patchEnvText rejects newline injection`() {
        try {
            patchEnvText(
                raw = "AXON_COLLECTION=old\n",
                values = mapOf("AXON_COLLECTION" to "axon\nEVIL=true"),
                dirtyKeys = setOf("AXON_COLLECTION"),
            )
            fail("Expected newline values to be rejected")
        } catch (expected: IllegalArgumentException) {
            assertTrue(expected.message.orEmpty().contains("newlines"))
        }
    }

    @Test fun `patchConfigTomlText preserves unmodeled TOML and patches dirty keys only`() {
        val raw = """
            [providers.vector]
            hybrid-enabled = true
            private-key = "keep"

            [custom.section]
            value = "keep"
        """.trimIndent() + "\n"

        val patched = patchConfigTomlText(
            raw = raw,
            values = mapOf(
                "providers.vector.hybrid-enabled" to "false",
                "providers.vector.hnsw-ef" to "64",
            ),
            dirtyKeys = setOf("providers.vector.hybrid-enabled"),
        )

        assertTrue(patched.contains("hybrid-enabled = false"))
        assertTrue(patched.contains("private-key = \"keep\""))
        assertTrue(patched.contains("[custom.section]"))
        assertTrue(patched.contains("value = \"keep\""))
        assertTrue(!patched.contains("hnsw-ef"))
    }

    @Test fun `patchConfigTomlText appends dirty known keys missing from raw file`() {
        val patched = patchConfigTomlText(
            raw = "[custom]\nvalue = \"keep\"\n",
            values = mapOf("ask.cache.ttl-secs" to "900"),
            dirtyKeys = setOf("ask.cache.ttl-secs"),
        )

        assertEquals(
            """
                [custom]
                value = "keep"

                [ask.cache]
                ttl-secs = 900
            """.trimIndent() + "\n",
            patched,
        )
    }
}
