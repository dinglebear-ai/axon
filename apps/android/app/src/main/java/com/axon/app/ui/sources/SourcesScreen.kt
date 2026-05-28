package com.axon.app.ui.sources

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Storage
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import android.net.Uri
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.SourceEntryUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraItem
import tv.tootie.aurora.components.AuroraProgress
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraStatCard

@Composable
fun SourcesScreen(vm: SourcesViewModel = viewModel()) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        Text("Sources", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()

        when (val state = uiState) {
            is SourcesUiState.Loading -> {
                AuroraProgress(modifier = Modifier.fillMaxWidth())
            }
            is SourcesUiState.Empty -> {
                EmptyContent(
                    title = "No sources yet",
                    description = "Use Ask or Search to start indexing content",
                    icon = Icons.Filled.Storage,
                    actionLabel = "Refresh",
                    onAction = vm::load,
                    modifier = Modifier.fillMaxWidth().padding(top = 16.dp),
                )
            }
            is SourcesUiState.Error -> ErrorContent(message = state.message, onRetry = vm::load)
            is SourcesUiState.Loaded -> {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    AuroraCard(
                        modifier = Modifier.weight(1f),
                        variant = AuroraCardVariant.Outlined,
                    ) {
                        AuroraStatCard(
                            label = "Sources",
                            value = "${state.sources.size}",
                        )
                    }
                    AuroraCard(
                        modifier = Modifier.weight(1f),
                        variant = AuroraCardVariant.Outlined,
                    ) {
                        AuroraStatCard(
                            label = "Chunks",
                            value = "${state.total}",
                        )
                    }
                }
                AuroraSeparator()
                LazyColumn(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    items(state.sources, key = { it.url }) { entry ->
                        SourceRow(entry)
                    }
                }
            }
        }
    }
}

@Composable
private fun SourceRow(entry: SourceEntryUi) {
    val uriHandler = LocalUriHandler.current
    val domain = runCatching {
        java.net.URI(entry.url).host?.removePrefix("www.") ?: entry.url
    }.getOrDefault(entry.url)

    AuroraItem(
        title = entry.url,
        description = domain,
        trailingContent = {
            Text(
                "${entry.chunks} chunks",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
        onClick = {
            val scheme = Uri.parse(entry.url).scheme
            if (scheme == "https" || scheme == "http") {
                runCatching { uriHandler.openUri(entry.url) }
            }
        },
    )
}
