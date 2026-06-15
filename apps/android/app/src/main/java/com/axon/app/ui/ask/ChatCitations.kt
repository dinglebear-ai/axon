package com.axon.app.ui.ask

import java.net.URI

/** A resolvable inline citation: the 1-based display number shown in the badge + the doc URL it opens. */
internal data class CitationLink(val displayIndex: Int, val url: String) {
    init {
        require(displayIndex >= 1) { "CitationLink.displayIndex must be 1-based (>= 1), was $displayIndex" }
    }
}

/** One parsed line from the answer's Sources block: backend `[Sn]` number (if any), URL, and a short title. */
internal data class AnswerSource(val num: Int?, val url: String, val title: String)

internal val sourceNumRegex = Regex("\\[S(\\d+)\\]")

internal val inlineCitationMarkerRegex = Regex("\\[S(\\d+)\\]")

internal val sourceUrlRegex = Regex("https?://[^\\s)\\]\"']+")

internal val answerMetadataBlockRegex = Regex(
    "(?is)\\n*#{1,3}\\s*(?:Citation\\s+Validation\\s+Failed|Retrieved\\s+Sources|Sources)\\s*\\n.*$",
)

/** Parse the trailing Sources block into ordered, de-duplicated sources with their backend numbers. */
internal fun parseAnswerSources(text: String): List<AnswerSource> {
    val block = answerMetadataBlockRegex.find(text)?.value ?: return emptyList()
    return block.lineSequence()
        .mapNotNull { line ->
            val url = sourceUrlRegex.find(line)?.value?.trimEnd('.', ',', ')', '"', '\'') ?: return@mapNotNull null
            val num = sourceNumRegex.find(line)?.groupValues?.get(1)?.toIntOrNull()
            AnswerSource(num = num, url = url, title = sourceDisplayTitle(url))
        }
        .distinctBy { it.url }
        .take(12)
        .toList()
}

/**
 * Map each backend source number (`[S15]`) to its 1-based display index + URL so inline
 * citations renumber to match the carousel (1..k) and link to the right doc, even when the
 * backend's source numbering isn't a contiguous 1..k. Sources without a backend `[Sn]` number
 * are excluded from the map (they still render in the carousel).
 */
internal fun buildCitationLinks(sources: List<AnswerSource>): Map<Int, CitationLink> =
    sources.mapIndexedNotNull { i, s -> s.num?.let { n -> n to CitationLink(i + 1, s.url) } }.toMap()

internal fun stripCitationText(text: String): String =
    text
        .replace(answerMetadataBlockRegex, "")
        .replace(Regex("\\s*\\[S\\d+\\]"), "")
        .trim()

/** Strip only the trailing sources/metadata block, leaving inline `[Sn]` markers intact. */
internal fun stripSourcesBlock(text: String): String =
    text.replace(answerMetadataBlockRegex, "").trim()

internal fun compactSourceLabel(url: String): String =
    runCatching {
        val withoutScheme = url.substringAfter("://")
        val host = withoutScheme.substringBefore("/")
        host.removePrefix("www.").substringBeforeLast(".").takeIf { it.isNotBlank() } ?: host
    }.getOrElse { "source" }

/** Carousel-facing title: the last path segment (e.g. `scrape.md`), else the host stem. */
internal fun sourceDisplayTitle(url: String): String =
    runCatching {
        URI(url).path.orEmpty().trim('/').split('/').lastOrNull { it.isNotBlank() }
    }.getOrNull()?.takeIf { it.isNotBlank() } ?: compactSourceLabel(url)
