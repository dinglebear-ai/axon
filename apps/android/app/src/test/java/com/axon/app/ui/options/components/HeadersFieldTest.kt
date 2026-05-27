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
}
