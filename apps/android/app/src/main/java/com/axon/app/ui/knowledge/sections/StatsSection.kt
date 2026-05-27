package com.axon.app.ui.knowledge.sections

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.common.chunkDocument
import com.axon.app.ui.knowledge.KnowledgeViewModel
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant

private val prettyJson = Json { prettyPrint = true }

/**
 * Stats tab — renders the raw `/v1/stats` payload as pretty-printed JSON.
 *
 * **R4 — chunked LazyColumn.** Stats payloads can be many KB (per-collection
 * point counts, vector dims, named-mode shape). A single `Text` inside a
 * scrollable Column blows up the slot table and stalls scrolling on phones.
 * We feed the rendered JSON through [chunkDocument] (paragraph → line → hard
 * char split at [com.axon.app.ui.common.DOC_CHUNK_TARGET_CHARS]) and emit one
 * `Text` per chunk inside a `LazyColumn` so only visible chunks compose.
 */
@Composable
fun StatsSection(vm: KnowledgeViewModel) {
    val state by vm.stats.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { vm.loadStats() }

    when (val s = state) {
        Resource.Idle, Resource.Loading -> LoadingContent(
            label = "Loading stats…",
            modifier = Modifier.fillMaxWidth(),
        )
        is Resource.Error -> ErrorContent(message = s.message, onRetry = { vm.loadStats(force = true) })
        is Resource.Ready<*> -> {
            @Suppress("UNCHECKED_CAST")
            val payload = (s as Resource.Ready<JsonElement>).value
            val chunks = remember(payload) {
                chunkDocument(prettyJson.encodeToString(JsonElement.serializer(), payload))
            }
            AuroraCard(
                modifier = Modifier.fillMaxSize(),
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
