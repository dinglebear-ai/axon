package com.axon.app.ui.document

import com.axon.app.data.repository.RetrieveResultUi

internal fun Int.formatCount(): String = "%,d".format(this)

internal enum class DocumentBlockKind { Heading, Body, Code }

internal data class DocumentBlock(
    val key: String,
    val kind: DocumentBlockKind,
    val text: String,
)

internal fun visibleDocumentWarnings(truncated: Boolean, warnings: List<String>): List<String> =
    warnings.filterNot { warning ->
        truncated && warning.contains("truncated", ignoreCase = true)
    }

internal fun markdownBlocks(markdown: String): List<DocumentBlock> {
    sitemapBlocks(markdown).takeIf { it.isNotEmpty() }?.let { return it }

    val blocks = mutableListOf<DocumentBlock>()
    val paragraph = mutableListOf<String>()
    var inCode = false
    val code = mutableListOf<String>()

    fun flushParagraph() {
        val text = paragraph.joinToString(" ")
            .replace(Regex("\\s+"), " ")
            .trim()
        paragraph.clear()
        if (text.isNotBlank()) blocks += DocumentBlock("body-${blocks.size}", DocumentBlockKind.Body, cleanInlineMarkdown(text))
    }
    fun flushCode() {
        val text = code.joinToString("\n")
            .trim()
            .trim('`')
            .trim()
        code.clear()
        if (text.isNotBlank()) blocks += DocumentBlock("code-${blocks.size}", DocumentBlockKind.Code, text)
    }

    markdown.lineSequence().forEach { rawLine ->
        val line = rawLine.trim()
        when {
            line.startsWith("```") -> {
                if (inCode) {
                    flushCode()
                    inCode = false
                } else {
                    flushParagraph()
                    inCode = true
                }
            }
            inCode -> code += rawLine.trimEnd()
            line.isBlank() -> flushParagraph()
            line.startsWith("#") -> {
                flushParagraph()
                val heading = line.trimStart('#').trim()
                if (heading.isNotBlank() && !heading.isSchemaBreadcrumb()) {
                    blocks += DocumentBlock("heading-${blocks.size}", DocumentBlockKind.Heading, cleanInlineMarkdown(heading))
                }
            }
            line.startsWith("- ") || line.startsWith("* ") -> paragraph += "• ${line.drop(2).trim()}"
            else -> paragraph += line
        }
    }
    flushParagraph()
    if (inCode) flushCode()
    return blocks
        .filter { it.text.isNotBlank() }
        .flatMap { it.expandSchemaObjects() }
        .flatMap { it.splitLongBlock() }
        .distinctBy { it.kind to it.text.lowercase() }
        .take(80)
}

private fun sitemapBlocks(markdown: String): List<DocumentBlock> {
    val urlMatches = Regex("""https?://\S+""").findAll(markdown).toList()
    if (urlMatches.size < 6 || !markdown.contains("weekly", ignoreCase = true) && !markdown.contains("daily", ignoreCase = true)) {
        return emptyList()
    }

    val normalized = markdown.replace(Regex("\\s+"), " ").trim()
    val matches = Regex("""https?://\S+""").findAll(normalized).toList()
    if (matches.size < 6) return emptyList()

    val blocks = mutableListOf<DocumentBlock>(
        DocumentBlock("heading-sitemap-entries", DocumentBlockKind.Heading, "Sitemap entries"),
    )
    matches.take(10).forEachIndexed { index, match ->
        val url = match.value.trimEnd(',', ';').let { if (it.length > 72) it.take(69).trimEnd('/', '-') + "..." else it }
        val nextStart = matches.getOrNull(index + 1)?.range?.first ?: normalized.length
        val metadata = normalized
            .substring(match.range.last + 1, nextStart)
            .replace(url, "")
            .trim()
        val updated = Regex("""\d{4}-\d{2}-\d{2}T[^\s]+""").find(metadata)?.value
        val frequency = Regex("""\b(always|hourly|daily|weekly|monthly|yearly|never)\b""", RegexOption.IGNORE_CASE)
            .find(metadata)
            ?.value
            ?.lowercase()
        val priority = Regex("""\b(?:0(?:\.\d+)?|1(?:\.0)?)\b""").findAll(metadata).lastOrNull()?.value
        val details = buildList {
            if (updated != null) add("updated $updated")
            if (frequency != null) add(frequency)
            if (priority != null) add("priority $priority")
        }
        blocks += DocumentBlock(
            key = "sitemap-entry-$index",
            kind = DocumentBlockKind.Body,
            text = if (details.isEmpty()) url else "• $url · ${details.joinToString(" · ")}",
        )
    }
    return blocks
}

private fun cleanInlineMarkdown(value: String): String =
    value
        .replace(Regex("""\[(.+?)]\((.+?)\)"""), "$1")
        .replace(Regex("""\s*\]?\(<#.*?>\)"""), "")
        .replace(Regex("""\s*\([^\n]*#.*?>\)"""), "")
        .replace(Regex("""\s*\]?\(<#[^\n]*"""), "")
        .replace(Regex("""\s*\([^\n]*#\([^\n]*"""), "")
        .replace(Regex("""\s*\[[^\]]*(resource|model|schema|property|member|variant)[^\]]*>\)"""), "")
        .replace(Regex("""object\s*\{([^}]*)\}"""), "object fields: $1")
        .replace(Regex("""\{([^}]*)\}"""), "$1")
        .replace("JSON string", "structured argument string", ignoreCase = true)
        .replace("\\_", "_")
        .replace("**", "")
        .replace("__", "")
        .replace("`", "")
        .replace("[", "")
        .replace("]", "")
        .replace("{", "")
        .replace("}", "")
        .replace(Regex("""(^|\s)[.,;:]\s+"""), "$1")
        .replace(Regex("\\s+"), " ")
        .trimStart('.', ',', ';', ':', ' ', '\t')
        .trim()

private fun DocumentBlock.expandSchemaObjects(): List<DocumentBlock> {
    if (kind != DocumentBlockKind.Body || !text.contains(" object fields:")) return listOf(this)
    val matches = Regex("""\b([A-Z][A-Za-z0-9]+)\s+object fields:""").findAll(text).toList()
    if (matches.isEmpty()) return listOf(this)

    val expanded = mutableListOf<DocumentBlock>()
    val prefix = text.substring(0, matches.first().range.first).trim()
    if (prefix.isNotBlank()) expanded += copy(key = "$key-prefix", text = prefix)

    matches.forEachIndexed { index, match ->
        val title = match.groupValues[1]
        val start = match.range.last + 1
        val end = matches.getOrNull(index + 1)?.range?.first ?: text.length
        val segment = "object fields: " + text.substring(start, end).trim()
        if (segment.isNotBlank()) {
            expanded += DocumentBlock("$key-schema-heading-$index", DocumentBlockKind.Heading, title)
            expanded += DocumentBlock("$key-schema-body-$index", DocumentBlockKind.Body, segment.formatSchemaObjectBody())
        }
    }
    return expanded.ifEmpty { listOf(this) }
}

private fun String.formatSchemaObjectBody(): String {
    val compact = replace(Regex("\\s+"), " ").trim()
    val fieldStart = Regex("""\b(?!fields\b)[a-z][A-Za-z0-9_]*:\s+""").find(compact)?.range?.first ?: return compact
    val intro = compact.substring(0, fieldStart).trim()
    val fields = compact
        .substring(fieldStart)
        .replace(Regex("""\s+(\b(?!fields\b)[a-z][A-Za-z0-9_]*:\s+)"""), "\n$1")
        .trim()
    return listOf(intro, fields).filter { it.isNotBlank() }.joinToString("\n")
}

private fun String.isSchemaBreadcrumb(): Boolean =
    contains("(resource)", ignoreCase = true) &&
        contains("(schema)", ignoreCase = true) &&
        contains(">")

internal fun documentTitle(result: RetrieveResultUi, blocks: List<DocumentBlock>): String =
    blocks.firstOrNull {
        it.kind == DocumentBlockKind.Heading &&
            !it.key.contains("schema-heading") &&
            !it.text.contains("(resource)", ignoreCase = true) &&
            !it.text.contains("(schema)", ignoreCase = true) &&
            !it.text.contains(">", ignoreCase = true)
    }?.text
        ?: titleFromUrl(result.matchedUrl ?: result.requestedUrl)

internal fun shortUrl(url: String): String =
    url
        .removePrefix("https://")
        .removePrefix("http://")
        .trimEnd('/')
        .let { if (it.length > 58) it.take(55).trimEnd() + "..." else it }

private fun titleFromUrl(url: String): String {
    val path = url
        .substringAfter("://", url)
        .substringAfter("/", "")
        .substringBefore("?")
        .trim('/')
    val parts = path
        .split('/')
        .filter { it.isNotBlank() }
        .takeLast(3)
        .flatMap { it.split('-', '_') }
        .filter { it.isNotBlank() && it.length > 1 }
    return parts
        .joinToString(" ") { part -> part.replaceFirstChar { if (it.isLowerCase()) it.titlecase() else it.toString() } }
        .ifBlank { shortUrl(url) }
}

private fun DocumentBlock.splitLongBlock(maxChars: Int = 900): List<DocumentBlock> {
    if (text.length <= maxChars || kind == DocumentBlockKind.Code) return listOf(this)
    val parts = mutableListOf<String>()
    val buffer = StringBuilder()
    text.split(Regex("(?<=[.!?])\\s+")).forEach { sentence ->
        if (buffer.isNotEmpty() && buffer.length + sentence.length > maxChars) {
            parts += buffer.toString().trim()
            buffer.clear()
        }
        if (buffer.isNotEmpty()) buffer.append(' ')
        buffer.append(sentence)
    }
    if (buffer.isNotBlank()) parts += buffer.toString().trim()
    return parts.mapIndexed { index, part -> copy(key = "$key-$index", text = part) }
}
