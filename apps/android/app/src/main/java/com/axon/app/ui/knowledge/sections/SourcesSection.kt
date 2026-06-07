package com.axon.app.ui.knowledge.sections

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Storage
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.data.repository.SourceEntryUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.knowledge.KnowledgeSourceRow
import com.axon.app.ui.knowledge.KnowledgeViewModel
import com.axon.app.ui.theme.AxonTheme
import java.net.URI

@Composable
fun SourcesSection(
    vm: KnowledgeViewModel,
    onOpenDocument: (String) -> Unit,
) {
    val state by vm.sources.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { vm.loadSources() }

    when (val s = state) {
        Resource.Idle, Resource.Loading -> LoadingContent(
            label = "Loading sources…",
            modifier = Modifier.fillMaxWidth(),
        )
        is Resource.Error -> ErrorContent(message = s.message, onRetry = { vm.loadSources(force = true) })
        is Resource.Ready -> {
            val entries = s.value
            if (entries.isEmpty()) {
                EmptyContent(
                    title = "No sources indexed",
                    description = "Use Ask, Search, or Ingest to populate the knowledge base.",
                    icon = Icons.Outlined.Storage,
                    modifier = Modifier.fillMaxWidth(),
                )
            } else {
                LazyColumn(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(horizontal = 6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    item {
                        Text(
                            "${entries.size} documents · ${entries.sumOf { it.chunks }.formatCount()} chunks",
                            color = AxonTheme.colors.textMuted,
                            fontSize = 11.sp,
                            lineHeight = 14.sp,
                            fontFamily = AxonTheme.fonts.mono,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 2.dp, vertical = 1.dp),
                        )
                    }
                    items(entries, key = { it.url }) { entry ->
                        KnowledgeSourceRow(
                            title = sourceTitle(entry.url),
                            domain = sourceDomain(entry.url),
                            source = "crawl",
                            chunks = entry.chunks,
                            onClick = { onOpenDocument(entry.url) },
                        )
                    }
                }
            }
        }
    }
}

private fun Int.formatCount(): String = "%,d".format(this)

private fun sourceDomain(url: String): String =
    runCatching { URI(url).host?.removePrefix("www.") }
        .getOrNull()
        ?.takeIf { it.isNotBlank() }
        ?: url.removePrefix("https://").removePrefix("http://").substringBefore("/")

private fun sourceTitle(url: String): String {
    val path = runCatching { URI(url).path.orEmpty() }.getOrElse { "" }.trim('/')
    val pieces = path
        .split('/')
        .filter { it.isNotBlank() }
        .takeLast(2)
        .flatMap { it.split('-', '_') }
        .filter { it.length > 1 }
    return pieces
        .joinToString(" ") { piece -> piece.replaceFirstChar { if (it.isLowerCase()) it.titlecase() else it.toString() } }
        .ifBlank { sourceDomain(url) }
}
