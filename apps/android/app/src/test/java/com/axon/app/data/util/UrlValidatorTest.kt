package com.axon.app.data.util

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class UrlValidatorTest {
    @Test fun `accepts http URL`()  = assertTrue(UrlValidator.isValidHttpUrl("http://example.com"))
    @Test fun `accepts https URL`() = assertTrue(UrlValidator.isValidHttpUrl("https://example.com/path?q=1"))
    @Test fun `rejects file scheme`()       = assertFalse(UrlValidator.isValidHttpUrl("file:///etc/passwd"))
    @Test fun `rejects ftp scheme`()        = assertFalse(UrlValidator.isValidHttpUrl("ftp://example.com"))
    @Test fun `rejects javascript scheme`() = assertFalse(UrlValidator.isValidHttpUrl("javascript:alert(1)"))
    @Test fun `rejects empty string`()      = assertFalse(UrlValidator.isValidHttpUrl(""))
    @Test fun `rejects malformed URL`()     = assertFalse(UrlValidator.isValidHttpUrl("not a url"))
    @Test fun `rejects no scheme`()         = assertFalse(UrlValidator.isValidHttpUrl("example.com"))

    // ── hostOrNull ────────────────────────────────────────────────────────────

    @Test fun `hostOrNull returns lowercased host for valid https URL`() {
        assertEquals("example.com", UrlValidator.hostOrNull("HTTPS://Example.COM/path"))
    }

    @Test fun `hostOrNull lowercases mixed-case host`() {
        assertEquals("github.com", UrlValidator.hostOrNull("https://GitHub.com/owner/repo"))
    }

    @Test fun `hostOrNull strips userinfo from host`() {
        // URL.host returns "github.com" — userinfo is on URL.userInfo, not host —
        // so a `https://evil@github.com` URL cannot smuggle a different host
        // through the validator.
        assertEquals("github.com", UrlValidator.hostOrNull("https://evil@github.com/x"))
    }

    @Test fun `hostOrNull returns null for non-URL input`() {
        assertNull(UrlValidator.hostOrNull("not a url"))
    }

    @Test fun `hostOrNull returns null for git ssh-style target`() {
        // R13 contract: non-URL forms return null so callers defer to
        // server-side validation rather than blocking valid git@ inputs.
        assertNull(UrlValidator.hostOrNull("git@github.com:owner/repo.git"))
    }

    @Test fun `hostOrNull returns null for owner-repo shorthand`() {
        assertNull(UrlValidator.hostOrNull("owner/repo"))
    }

    @Test fun `hostOrNull returns null for file scheme`() {
        assertNull(UrlValidator.hostOrNull("file:///etc/passwd"))
    }

    @Test fun `hostOrNull returns null for blank input`() {
        assertNull(UrlValidator.hostOrNull(""))
    }

    @Test fun `hostOrNull handles ipv4 literal`() {
        assertEquals("192.168.1.1", UrlValidator.hostOrNull("https://192.168.1.1/x"))
    }

    @Test fun `hostOrNull preserves subdomain for lookalike detection`() {
        // Catches `github.com.attacker.com` lookalikes — the host is the FULL
        // hostname, not a substring, so endsWith(".github.com") below correctly
        // rejects this.
        val host = UrlValidator.hostOrNull("https://github.com.attacker.com/x")
        assertEquals("github.com.attacker.com", host)
        assertFalse(host == "github.com")
        assertFalse(host!!.endsWith(".github.com"))
    }
}
