package com.axon.app.ui.ask

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraPromptInput
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable
fun AskScreen(vm: AskViewModel = viewModel()) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()
    val history by vm.history.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Ask Axon", style = MaterialTheme.typography.headlineMedium)

        when (val state = uiState) {
            is AskUiState.Loading -> LoadingContent(label = "Searching knowledge base…")
            is AskUiState.Success -> {
                AuroraCard(
                    modifier = Modifier.fillMaxWidth(),
                    variant = AuroraCardVariant.Filled,
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(
                            "Q: ${state.result.query}",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Text(state.result.answer, style = MaterialTheme.typography.bodyMedium)
                        state.result.timingMs?.let { ms ->
                            Text(
                                "${ms}ms",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
            is AskUiState.Error -> ErrorContent(message = state.message)
            is AskUiState.Idle -> {}
        }

        Spacer(Modifier.weight(1f))

        AnimatedVisibility(visible = history.isNotEmpty() && uiState is AskUiState.Idle) {
            Column {
                Text(
                    "Recent",
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(4.dp))
                LazyColumn(
                    modifier = Modifier.heightIn(max = 220.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    items(history, key = { it.id }) { entry ->
                        HistoryCard(entry = entry, onClick = { input = entry.query })
                    }
                }
            }
        }

        AuroraPromptInput(
            value = input,
            onValueChange = { input = it },
            onSend = {
                vm.ask(input)
                input = ""
            },
            placeholder = "Ask anything about your indexed knowledge…",
            loading = uiState is AskUiState.Loading,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun HistoryCard(entry: AskHistoryEntry, onClick: () -> Unit) {
    val fmt = remember { SimpleDateFormat("HH:mm", Locale.getDefault()) }
    AuroraCard(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
        variant = AuroraCardVariant.Outlined,
    ) {
        Column(modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp)) {
            Text(entry.query, style = MaterialTheme.typography.bodySmall, maxLines = 1)
            Text(
                fmt.format(Date(entry.askedAt)),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
