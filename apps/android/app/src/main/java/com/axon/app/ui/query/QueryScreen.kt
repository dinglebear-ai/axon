package com.axon.app.ui.query

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.nav.LocalOpenDocument
import com.axon.app.ui.operations.modeOptionsCog
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.QueryHitUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraPromptInput
import tv.tootie.aurora.components.AuroraSeparator

@Composable
fun QueryScreen(vm: QueryViewModel = viewModel()) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        Text("Vector Query", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()

        when (val state = uiState) {
            is QueryUiState.Loading -> LoadingContent(
                label = "Searching vectors…",
                modifier = Modifier.weight(1f),
            )
            is QueryUiState.Results -> {
                if (state.hits.isEmpty()) {
                    EmptyContent(
                        title = "No results",
                        description = "No matching documents found. Try a different query.",
                        icon = Icons.Filled.Search,
                        modifier = Modifier.weight(1f).fillMaxWidth(),
                    )
                } else {
                    LazyColumn(
                        modifier = Modifier.weight(1f),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        items(state.hits, key = { h -> "${h.url}#${h.rank}" }) { hit ->
                            QueryHitCard(hit)
                        }
                    }
                }
            }
            is QueryUiState.Error -> {
                ErrorContent(message = state.message)
                Spacer(Modifier.weight(1f))
            }
            is QueryUiState.Empty -> {
                EmptyContent(
                    title = "No results",
                    description = "No matching documents found. Try a different query.",
                    icon = Icons.Filled.Search,
                    modifier = Modifier.weight(1f).fillMaxWidth(),
                )
            }
            is QueryUiState.Idle -> {
                EmptyContent(
                    title = "Query your knowledge",
                    description = "Search your indexed knowledge using semantic vector similarity",
                    icon = Icons.Filled.Search,
                    modifier = Modifier.weight(1f).fillMaxWidth(),
                )
            }
        }

        AuroraSeparator()
        AuroraPromptInput(
            value = input,
            onValueChange = { input = it },
            onSend = { vm.query(input) },
            placeholder = "Query indexed knowledge…",
            loading = uiState is QueryUiState.Loading,
            actionLeft = modeOptionsCog(),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun QueryHitCard(hit: QueryHitUi) {
    val openDocument = LocalOpenDocument.current
    AuroraCard(
        onClick = { openDocument(hit.url) },
        modifier = Modifier.fillMaxWidth(),
        variant = AuroraCardVariant.Elevated,
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(
                    hit.source,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    "#${hit.rank}  %.3f".format(hit.score),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            AuroraSeparator()
            Text(hit.snippet, style = MaterialTheme.typography.bodySmall, maxLines = 3)
        }
    }
}
