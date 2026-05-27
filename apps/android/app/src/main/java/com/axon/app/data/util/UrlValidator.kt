package com.axon.app.data.util

import java.net.URL

/**
 * Client-side fail-fast URL guard. Server-side SSRF protection in
 * src/core/http/ssrf.rs remains the security backstop; this helper only
 * rejects obviously bad inputs (file://, javascript:, malformed) before
 * the network call.
 */
object UrlValidator {
    fun isValidHttpUrl(input: String): Boolean {
        if (input.isBlank()) return false
        val url = runCatching { URL(input) }.getOrNull() ?: return false
        return url.protocol == "http" || url.protocol == "https"
    }
}
