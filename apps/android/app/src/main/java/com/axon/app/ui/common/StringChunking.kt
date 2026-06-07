package com.axon.app.ui.common

/** Target size (chars) for each rendered chunk in long-document `LazyColumn`s. */
internal const val DOC_CHUNK_TARGET_CHARS = 2_000

/**
 * Split a string into bounded blocks for `LazyColumn` rendering. Splits at
 * paragraph (`\n\n`) boundaries first; oversized paragraphs are then split at
 * line (`\n`) boundaries; anything still over the target is sliced by char so
 * a single 10K paragraph never becomes a single `Text` node.
 *
 * Used by document-style screens for retrieved, scraped, or summarized text.
 */
internal fun chunkDocument(content: String): List<String> {
    if (content.length <= DOC_CHUNK_TARGET_CHARS) return listOf(content)
    val out = ArrayList<String>()
    val buf = StringBuilder()
    fun flush() {
        if (buf.isNotEmpty()) {
            out += buf.toString()
            buf.clear()
        }
    }
    fun appendUnit(unit: String, sep: String) {
        if (buf.isNotEmpty()) {
            buf.append(sep)
            if (buf.length + unit.length > DOC_CHUNK_TARGET_CHARS) flush()
        }
        buf.append(unit)
    }
    for (paragraph in content.split("\n\n")) {
        if (paragraph.length <= DOC_CHUNK_TARGET_CHARS) {
            appendUnit(paragraph, "\n\n")
            continue
        }
        flush()
        for (line in paragraph.split("\n")) {
            if (line.length <= DOC_CHUNK_TARGET_CHARS) {
                appendUnit(line, "\n")
            } else {
                flush()
                var i = 0
                while (i < line.length) {
                    val end = (i + DOC_CHUNK_TARGET_CHARS).coerceAtMost(line.length)
                    out += line.substring(i, end)
                    i = end
                }
            }
        }
        flush()
    }
    flush()
    return out
}
