package com.axon.app.data.util

import java.net.URL

/**
 * Client-side fail-fast URL guard. Server-side SSRF protection in
 * src/core/http/ssrf.rs remains the security backstop; this helper only
 * rejects obviously bad inputs (file://, javascript:, malformed) before
 * the network call.
 */
object UrlValidator {
    fun isValidHttpUrl(input: String): Boolean = parseHttpUrl(input) != null

    /**
     * Returns the lowercased host of [input] when it parses as a valid http(s)
     * URL. Used by callers that need both the validity guard AND the host (e.g.
     * source target validation). Non-URL inputs (git@host:
     * ssh form, owner/repo shorthand, etc.) return null — callers can decide
     * whether to accept those or defer to server-side validation.
     */
    fun hostOrNull(input: String): String? = parseHttpUrl(input)?.host?.lowercase()

    private fun parseHttpUrl(input: String): URL? {
        if (input.isBlank()) return null
        val url = runCatching { URL(input) }.getOrNull() ?: return null
        return if (url.protocol == "http" || url.protocol == "https") url else null
    }
}
