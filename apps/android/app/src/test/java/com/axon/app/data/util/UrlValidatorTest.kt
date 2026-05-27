package com.axon.app.data.util

import org.junit.Assert.assertFalse
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
}
