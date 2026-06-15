package com.axon.app.ui.ask

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import java.io.ByteArrayOutputStream

/** A text file attached to a prompt: its display name and (capped) content. */
internal data class PromptAttachment(
    val name: String,
    val content: String,
    val truncated: Boolean,
)

private const val MAX_ATTACH_BYTES = 512 * 1024
private const val MAX_ATTACH_CHARS = 16_000

/**
 * Reads a picked document as UTF-8 text, bounded in size, rejecting binaries.
 * The content is what gets attached to the next question; the server never
 * sees the file itself, only its text inlined into the query.
 */
internal fun readPromptAttachment(context: Context, uri: Uri): Result<PromptAttachment> = runCatching {
    val name = displayName(context, uri) ?: "attachment"
    val bytes = context.contentResolver.openInputStream(uri)?.use { stream ->
        val out = ByteArrayOutputStream()
        val buf = ByteArray(8192)
        var total = 0
        while (true) {
            val n = stream.read(buf)
            if (n < 0) break
            out.write(buf, 0, n)
            total += n
            if (total >= MAX_ATTACH_BYTES) break
        }
        out.toByteArray()
    } ?: error("Could not open “$name”")

    val text = bytes.toString(Charsets.UTF_8)
    require(text.isNotBlank()) { "“$name” is empty" }
    // Binary guard: NUL/low control bytes or UTF-8 replacement chars dominating.
    val suspicious = text.count { it == '�' || it.code < 9 }
    require(suspicious.toFloat() / text.length < 0.02f) { "“$name” isn’t a readable text file" }

    val truncated = bytes.size >= MAX_ATTACH_BYTES || text.length > MAX_ATTACH_CHARS
    PromptAttachment(
        name = name,
        content = if (text.length > MAX_ATTACH_CHARS) text.take(MAX_ATTACH_CHARS) else text,
        truncated = truncated,
    )
}

private fun displayName(context: Context, uri: Uri): String? =
    context.contentResolver
        .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
        ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
        ?.takeIf { it.isNotBlank() }
