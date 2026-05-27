package com.axon.app.ui.sources

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.SourceEntryUi
import com.axon.app.ui.common.ErrorContent
import tv.tootie.aurora.components.AuroraItem
import tv.tootie.aurora.components.AuroraProgress

@Composable
fun SourcesScreen(vm: SourcesViewModel = viewModel()) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        Text("Sources", style = MaterialTheme.typography.headlineMedium)
        Spacer(Modifier.height(8.dp))

        when (val state = uiState) {
            is SourcesUiState.Loading -> {
                AuroraProgress(modifier = Modifier.fillMaxWidth())
            }
            is SourcesUiState.Error -> ErrorContent(message = state.message)
            is SourcesUiState.Loaded -> {
                Text(
                    "${state.sources.size} sources · ${state.total} chunks",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(8.dp))
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
    // AuroraItem takes title: String and optional description + trailing composable
    AuroraItem(
        title = entry.url,
        trailingContent = {
            Text(
                "${entry.chunks}",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
        onClick = { runCatching { uriHandler.openUri(entry.url) } },
    )
}
