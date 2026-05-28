package com.axon.app.ui.system

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.common.chunkDocument
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraSeparator

private val prettyJson = Json { prettyPrint = true }

/**
 * System · Doctor — renders the raw `/v1/doctor` payload as pretty-printed JSON
 * with a manual refresh button. Stack and Config sub-sections are deferred.
 *
 * **R4 — chunked LazyColumn.** Doctor payloads include service tables, env
 * dumps, and embedded version blobs; a single `Text` blows up the slot table.
 * We feed the rendered JSON through [chunkDocument] and emit one `Text` per
 * chunk inside a `LazyColumn` so only visible chunks compose.
 */
@Composable
fun SystemScreen(vm: SystemViewModel = viewModel()) {
    val state by vm.doctor.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("System · Doctor", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()

        AuroraButton(
            onClick = { vm.refresh() },
            variant = AuroraButtonVariant.Outlined,
            modifier = Modifier.fillMaxWidth(),
        ) { Text("Refresh") }

        when (val s = state) {
            Resource.Idle, Resource.Loading -> LoadingContent(
                label = "Running doctor…",
                modifier = Modifier.fillMaxWidth(),
            )
            is Resource.Error -> ErrorContent(message = s.message, onRetry = { vm.refresh() })
            is Resource.Ready -> {
                val payload = s.value
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
}
