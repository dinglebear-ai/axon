package com.axon.app.feature.memory.sections

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
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
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.feature.memory.KnowledgeSourceRow
import com.axon.app.feature.memory.KnowledgeViewModel
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
                val reveal = rememberRevealState()
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = androidx.compose.foundation.layout.PaddingValues(top = 6.dp, bottom = 16.dp),
                    verticalArrangement = Arrangement.spacedBy(9.dp),
                ) {
                    item {
                        Text(
                            "${entries.size} documents · ${entries.sumOf { it.chunks }.formatCount()} chunks",
                            color = AxonTheme.colors.textMuted,
                            fontSize = 12.sp,
                            lineHeight = 15.sp,
                            fontFamily = AxonTheme.fonts.body,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 2.dp, vertical = 3.dp),
                        )
                    }
                    itemsIndexed(entries, key = { _, it -> it.url }) { index, entry ->
                        KnowledgeSourceRow(
                            title = sourceTitle(entry.url),
                            domain = sourceDomain(entry.url),
                            source = "crawl",
                            chunks = entry.chunks,
                            onClick = { onOpenDocument(entry.url) },
                            modifier = Modifier
                                .animateItem()
                                .revealOnce(reveal, entry.url, index),
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

/** Generic "whole-repo dump" filenames whose parent folder is the real identity. */
private val GENERIC_DOC_NAMES = setOf("repo", "index", "readme")

/**
 * Real, un-mangled source name. Shows the actual last path segment verbatim
 * (preserving real casing and separators) — and when that file is a generic
 * dump name (repo.md / index.md / readme.md), shows its parent folder instead,
 * which is the meaningful identifier. No title-casing, no invented names.
 */
private fun sourceTitle(url: String): String {
    val path = runCatching { URI(url).path.orEmpty() }.getOrElse { "" }.trim('/')
    val segments = path.split('/').filter { it.isNotBlank() }
    if (segments.isEmpty()) return sourceDomain(url)
    val file = segments.last()
    val stem = file.substringBeforeLast('.').lowercase()
    return if (stem in GENERIC_DOC_NAMES && segments.size >= 2) {
        segments[segments.size - 2]
    } else {
        file
    }
}
