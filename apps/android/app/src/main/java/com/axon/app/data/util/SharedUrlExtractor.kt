package com.axon.app.data.util

private val HttpUrlPattern = Regex("""https?://[^\s<>"'`]+""", RegexOption.IGNORE_CASE)

object SharedUrlExtractor {
    fun firstHttpUrl(input: CharSequence?): String? {
        if (input.isNullOrBlank()) return null
        return HttpUrlPattern.findAll(input)
            .map { match -> normalizeSharedUrl(match.value) }
            .firstOrNull { url -> UrlValidator.isValidHttpUrl(url) }
    }

    private fun normalizeSharedUrl(raw: String): String =
        raw.trim().trimEnd('.', ',', ';', ':', '!', '?', ')', ']', '}')
}
