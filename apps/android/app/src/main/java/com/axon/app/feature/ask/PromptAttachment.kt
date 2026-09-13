package com.axon.app.feature.ask

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream

/** A text file attached to a prompt: its display name, size, and (capped) content. */
internal data class PromptAttachment(
    val name: String,
    val content: String,
    val truncated: Boolean,
    val sizeBytes: Long,
) {
    init {
        require(sizeBytes >= 0) { "sizeBytes must be >= 0" }
    }
}

private const val MAX_ATTACH_BYTES = 512 * 1024
private const val MAX_ATTACH_CHARS = 16_000
internal const val MAX_ATTACHMENTS = 6

internal data class AttachmentAdmission(
    val accepted: List<PromptAttachment>,
    val failed: List<Throwable>,
    val skipped: Int,
)

internal suspend fun admitPromptAttachments(
    context: Context,
    uris: List<Uri>,
    existing: List<PromptAttachment>,
) = admitAttachmentCandidates(
    candidates = uris,
    existingNames = existing.map { it.name }.toSet(),
    availableSlots = (MAX_ATTACHMENTS - existing.size).coerceAtLeast(0),
    nameOf = { runCatching { withContext(Dispatchers.IO) { fileMeta(context, it).name } } },
    read = { withContext(Dispatchers.IO) { readPromptAttachment(context, it) } },
)

internal suspend fun <T> admitAttachmentCandidates(
    candidates: List<T>,
    existingNames: Set<String>,
    availableSlots: Int,
    nameOf: suspend (T) -> Result<String>,
    read: suspend (T) -> Result<PromptAttachment>,
): AttachmentAdmission {
    var slots = availableSlots.coerceAtLeast(0)
    val names = existingNames.toMutableSet()
    val accepted = mutableListOf<PromptAttachment>()
    val failed = mutableListOf<Throwable>()
    var skipped = 0
    for (candidate in candidates) {
        if (slots == 0) {
            skipped++
            continue
        }
        val nameResult = nameOf(candidate)
        if (nameResult.isFailure) {
            val error = checkNotNull(nameResult.exceptionOrNull())
            error.rethrowIfCancellation()
            failed += error
            continue
        }
        val name = nameResult.getOrThrow()
        if (!names.add(name)) {
            skipped++
            continue
        }
        read(candidate)
            .onSuccess {
                accepted += it
                slots--
            }.onFailure {
                it.rethrowIfCancellation()
                names.remove(name)
                failed += it
            }
    }
    return AttachmentAdmission(accepted, failed, skipped)
}

private fun Throwable.rethrowIfCancellation() {
    if (this is CancellationException) throw this
}

/** Human-readable byte size for attachment chips. */
internal fun formatBytes(bytes: Long): String =
    when {
        bytes < 0 -> ""
        bytes < 1024 -> "$bytes B"
        bytes < 1024 * 1024 -> "%.1f KB".format(bytes / 1024.0)
        else -> "%.1f MB".format(bytes / (1024.0 * 1024))
    }

/**
 * Reads a picked document as UTF-8 text, bounded in size, rejecting binaries.
 * The content is what gets attached to the next question; the server never
 * sees the file itself, only its text inlined into the query.
 */
internal fun readPromptAttachment(
    context: Context,
    uri: Uri,
): Result<PromptAttachment> =
    runCatching {
        val meta = fileMeta(context, uri)
        val name = meta.name
        val bytes =
            context.contentResolver.openInputStream(uri)?.use { stream ->
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
            sizeBytes = if (meta.size >= 0) meta.size else bytes.size.toLong(),
        )
    }

private data class FileMeta(
    val name: String,
    val size: Long,
)

private fun fileMeta(
    context: Context,
    uri: Uri,
): FileMeta =
    context.contentResolver
        .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)
        ?.use { cursor ->
            if (cursor.moveToFirst()) {
                val name = cursor.getString(0)?.takeIf { it.isNotBlank() } ?: "attachment"
                val size = if (!cursor.isNull(1)) cursor.getLong(1) else -1L
                FileMeta(name, size)
            } else {
                null
            }
        }
        ?: FileMeta("attachment", -1L)
