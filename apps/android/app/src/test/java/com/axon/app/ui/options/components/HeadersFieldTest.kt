package com.axon.app.ui.options.components

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pure-function tests for the R3 redaction logic inside [HeadersField]. The
 * Compose UI is not exercised — we test only `isSensitiveHeaderKey` and the
 * canonical Key:Value joiner so the redaction policy is unambiguous.
 */
class HeadersFieldTest {

    @Test fun `sensitive keys match case-insensitively`() {
        assertTrue(isSensitiveHeaderKey("Authorization"))
        assertTrue(isSensitiveHeaderKey("authorization"))
        assertTrue(isSensitiveHeaderKey("AUTHORIZATION"))
        assertTrue(isSensitiveHeaderKey("  Authorization  "))   // trims surrounding whitespace
        assertTrue(isSensitiveHeaderKey("Cookie"))
        assertTrue(isSensitiveHeaderKey("X-Api-Key"))
        assertTrue(isSensitiveHeaderKey("x-api-key"))
        assertTrue(isSensitiveHeaderKey("Proxy-Authorization"))
        assertTrue(isSensitiveHeaderKey("X-Auth-Token"))
    }

    @Test fun `non-sensitive keys are not redacted`() {
        assertFalse(isSensitiveHeaderKey("Content-Type"))
        assertFalse(isSensitiveHeaderKey("Accept"))
        assertFalse(isSensitiveHeaderKey("X-Trace-Id"))
        assertFalse(isSensitiveHeaderKey(""))
    }

    @Test fun `joinHeader returns Key colon space Value`() {
        assertEquals("Authorization: Bearer abc", joinHeader("Authorization", "Bearer abc"))
        assertEquals("X-Trace: 1", joinHeader("  X-Trace  ", "1"))
    }

    @Test fun `joinHeader returns null when key is blank`() {
        assertNull(joinHeader("", "value"))
        assertNull(joinHeader("   ", "value"))
    }

    // ── splitHeader ───────────────────────────────────────────────────────────

    @Test fun `splitHeader splits at first colon`() {
        assertEquals("Authorization" to "Bearer abc", splitHeader("Authorization: Bearer abc"))
        assertEquals("X-Trace" to "1", splitHeader("X-Trace: 1"))
    }

    @Test fun `splitHeader preserves colons in value`() {
        // Bearer tokens are JWTs with two periods, not colons, but timestamps,
        // URIs, and IPv6 addresses can contain colons — only the first one splits.
        assertEquals("X-Forwarded-For" to "::1", splitHeader("X-Forwarded-For: ::1"))
        assertEquals("X-Trace" to "session:abc:123", splitHeader("X-Trace: session:abc:123"))
    }

    @Test fun `splitHeader treats orphan input as value-only`() {
        // Preserves the user's typed text so they can rescue it by adding a key.
        assertEquals("" to "orphan", splitHeader("orphan"))
    }

    @Test fun `splitHeader handles colon-prefixed value`() {
        assertEquals("" to "value", splitHeader(":value"))
    }

    @Test fun `splitHeader trims surrounding whitespace from key and value`() {
        assertEquals("Authorization" to "Bearer abc", splitHeader("  Authorization  :  Bearer abc  "))
    }

    // ── HeadersReducer ────────────────────────────────────────────────────────

    @Test fun `init returns single blank row for empty list`() {
        assertEquals(listOf("" to ""), HeadersReducer.init(emptyList()))
    }

    @Test fun `init parses wire list into pairs`() {
        val rows = HeadersReducer.init(listOf("Authorization: Bearer x", "X-Trace: 1"))
        assertEquals(2, rows.size)
        assertEquals("Authorization" to "Bearer x", rows[0])
        assertEquals("X-Trace" to "1", rows[1])
    }

    @Test fun `setKey updates only the targeted row`() {
        val rows = listOf("A" to "1", "B" to "2", "C" to "3")
        val next = HeadersReducer.setKey(rows, 1, "B2")
        assertEquals(listOf("A" to "1", "B2" to "2", "C" to "3"), next)
    }

    @Test fun `setValue updates only the targeted row`() {
        val rows = listOf("A" to "1", "B" to "2")
        val next = HeadersReducer.setValue(rows, 0, "1-new")
        assertEquals(listOf("A" to "1-new", "B" to "2"), next)
    }

    @Test fun `setKey ignores out-of-bounds index`() {
        val rows = listOf("A" to "1")
        assertEquals(rows, HeadersReducer.setKey(rows, 99, "boom"))
        assertEquals(rows, HeadersReducer.setKey(rows, -1, "boom"))
    }

    @Test fun `addBlank appends an empty row`() {
        val rows = listOf("A" to "1")
        val next = HeadersReducer.addBlank(rows)
        assertEquals(listOf("A" to "1", "" to ""), next)
    }

    @Test fun `remove drops the row at index`() {
        val rows = listOf("A" to "1", "B" to "2", "C" to "3")
        val next = HeadersReducer.remove(rows, 1)
        assertEquals(listOf("A" to "1", "C" to "3"), next)
    }

    @Test fun `remove keeps one blank row when removing the last entry`() {
        val rows = listOf("A" to "1")
        val next = HeadersReducer.remove(rows, 0)
        assertEquals(listOf("" to ""), next)
    }

    @Test fun `toWire serializes non-blank rows and drops blank-key rows`() {
        val rows = listOf("A" to "1", "" to "stray", "B" to "")
        val wire = HeadersReducer.toWire(rows)
        // Blank-key rows are dropped; "B" with empty value is kept (the value is
        // a separate concern — some headers accept an empty value).
        assertEquals(listOf("A: 1", "B: "), wire)
    }
}
