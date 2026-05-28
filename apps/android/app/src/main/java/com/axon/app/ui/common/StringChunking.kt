package com.axon.app.ui.common

/** Target size (chars) for each rendered chunk in document/JSON `LazyColumn`s. */
internal const val DOC_CHUNK_TARGET_CHARS = 2_000

/**
 * Split a string into bounded blocks for `LazyColumn` rendering. Splits at
 * paragraph (`\n\n`) boundaries first; oversized paragraphs are then split at
 * line (`\n`) boundaries; anything still over the target is sliced by char so
 * a single 10K paragraph never becomes a single `Text` node.
 *
 * Used by DocumentScreen for retrieved documents and by StatsSection /
 * SystemScreen for raw JSON payloads (per R4).
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
        if (buf.isNotEmpty() && buf.length + sep.length + unit.length > DOC_CHUNK_TARGET_CHARS) {
            buf.append(sep)
            flush()
        } else if (buf.isNotEmpty()) {
            buf.append(sep)
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
