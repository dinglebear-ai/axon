package com.axon.app.ui.document

import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone

/** Target size (chars) for each rendered chunk in the document `LazyColumn`. */
internal const val DOC_CHUNK_TARGET_CHARS = 2_000

/**
 * Split a document into bounded blocks for `LazyColumn` rendering. Splits at
 * paragraph (`\n\n`) boundaries first; oversized paragraphs are then split at
 * line (`\n`) boundaries; anything still over the target is sliced by char so
 * a single 10K paragraph never becomes a single `Text` node.
 *
 * Exposed `internal` for unit tests.
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
        if (buf.isNotEmpty() && buf.length + sep.length + unit.length > DOC_CHUNK_TARGET_CHARS) flush()
        if (buf.isNotEmpty()) buf.append(sep)
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

@Composable
fun DocumentScreen(
    url: String,
    vm: DocumentViewModel = viewModel(),
) {
    LaunchedEffect(url) { vm.load(url) }
    val state by vm.uiState.collectAsStateWithLifecycle()
    val uriHandler = LocalUriHandler.current
    val context = LocalContext.current

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        when (val s = state) {
            is DocumentUiState.Loading -> LoadingContent(
                label = "Fetching document…",
                modifier = Modifier.fillMaxWidth(),
            )

            is DocumentUiState.Error -> ErrorContent(
                message = s.message,
                onRetry = { vm.retry(url) },
            )

            is DocumentUiState.Success -> {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        s.result.matchedUrl ?: s.result.requestedUrl,
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.weight(1f).padding(end = 8.dp),
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    AuroraStatusIndicator(
                        tone = AuroraStatusTone.Online,
                        label = "${s.result.chunkCount} chunks",
                    )
                }
                AuroraButton(
                    onClick = {
                        val target = s.result.matchedUrl ?: s.result.requestedUrl
                        runCatching { uriHandler.openUri(target) }.onFailure {
                            // No handler for http(s) intents — surface so the click isn't silent.
                            Toast.makeText(context, "No browser available to open the URL", Toast.LENGTH_SHORT).show()
                        }
                    },
                    variant = AuroraButtonVariant.Outlined,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Open source URL in browser") }

                if (s.result.truncated) {
                    AuroraCallout(
                        title = "Truncated",
                        message = "Document was truncated to fit the token budget; some chunks may be missing.",
                        variant = AuroraCalloutVariant.Warn,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                s.result.warnings.forEach { w ->
                    AuroraCallout(
                        title = "Warning",
                        message = w,
                        variant = AuroraCalloutVariant.Warn,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }

                AuroraSeparator()

                if (s.result.content.isBlank()) {
                    EmptyContent(
                        title = "Document is empty",
                        description = "The server returned a stored document with no rendered content.",
                        icon = Icons.Outlined.Description,
                        modifier = Modifier.fillMaxWidth(),
                    )
                } else {
                    val chunks = remember(s.result.content) { chunkDocument(s.result.content) }
                    AuroraCard(
                        modifier = Modifier.fillMaxWidth().weight(1f),
                        variant = AuroraCardVariant.Outlined,
                    ) {
                        SelectionContainer {
                            LazyColumn(
                                modifier = Modifier.fillMaxSize().padding(12.dp),
                                verticalArrangement = Arrangement.spacedBy(4.dp),
                            ) {
                                items(chunks.size, key = { it }) { i ->
                                    Text(chunks[i], style = MaterialTheme.typography.bodySmall)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
