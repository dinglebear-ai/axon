package com.axon.app.ui.settings

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
            AXON_OPENAI_MODEL=unchanged
        """.trimIndent() + "\n"

        val patched = patchEnvText(
            raw = raw,
            values = mapOf(
                "AXON_COLLECTION" to "new collection",
                "AXON_OPENAI_MODEL" to "should-not-write",
            ),
            dirtyKeys = setOf("AXON_COLLECTION"),
        )

        assertTrue(patched.contains("CUSTOM_SECRET=keep-me"))
        assertTrue(patched.contains("AXON_COLLECTION=\"new collection\""))
        assertTrue(patched.contains("AXON_OPENAI_MODEL=unchanged"))
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
            [search]
            collection = "old"
            private-key = "keep"

            [custom.section]
            value = "keep"
        """.trimIndent() + "\n"

        val patched = patchConfigTomlText(
            raw = raw,
            values = mapOf(
                "search.collection" to "new",
                "search.hybrid-enabled" to "false",
            ),
            dirtyKeys = setOf("search.collection"),
        )

        assertTrue(patched.contains("collection = \"new\""))
        assertTrue(patched.contains("private-key = \"keep\""))
        assertTrue(patched.contains("[custom.section]"))
        assertTrue(patched.contains("value = \"keep\""))
        assertTrue(!patched.contains("hybrid-enabled"))
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
