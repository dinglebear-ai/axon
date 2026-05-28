package com.axon.app.ui.ask

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.fab.FabLauncher
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraPromptInput
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable
fun AskScreen(
    onOpenDocument: (String) -> Unit = {},
    vm: AskViewModel = viewModel(),
) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()
    val history by vm.history.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }

    Box(modifier = Modifier.fillMaxSize()) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Ask Axon", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()

        when (val state = uiState) {
            is AskUiState.Loading -> LoadingContent(label = "Searching knowledge base…")
            is AskUiState.Streaming -> {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    AuroraStatusIndicator(
                        tone = AuroraStatusTone.Automating,
                        label = "Generating…",
                    )
                    AuroraCard(
                        modifier = Modifier.fillMaxWidth(),
                        variant = AuroraCardVariant.Filled,
                    ) {
                        Column(
                            modifier = Modifier.padding(16.dp),
                            verticalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            Text(
                                "Q: ${state.query}",
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            if (state.partialAnswer.isNotEmpty()) {
                                Text(state.partialAnswer, style = MaterialTheme.typography.bodyMedium)
                            } else {
                                Text(
                                    "Answering…",
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                }
            }
            is AskUiState.Success -> {
                AuroraCard(
                    modifier = Modifier.fillMaxWidth(),
                    variant = AuroraCardVariant.Filled,
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                        ) {
                            Text(
                                "Q: ${state.result.query}",
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                modifier = Modifier.weight(1f),
                                maxLines = 2,
                                overflow = TextOverflow.Ellipsis,
                            )
                            state.result.timingMs?.let { ms ->
                                AuroraStatusIndicator(
                                    tone = AuroraStatusTone.Online,
                                    label = "${ms}ms",
                                )
                            }
                        }
                        AuroraSeparator()
                        Text(state.result.answer, style = MaterialTheme.typography.bodyMedium)
                    }
                }
                state.historyWarning?.let { warning ->
                    Text(
                        warning,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
                    )
                }
            }
            is AskUiState.Error -> ErrorContent(message = state.message)
            is AskUiState.Idle -> {}
        }

        Spacer(Modifier.weight(1f))

        AnimatedVisibility(visible = history.isEmpty() && uiState is AskUiState.Idle) {
            EmptyContent(
                title = "Ask anything",
                description = "Ask anything about your indexed knowledge",
                icon = Icons.Outlined.AutoAwesome,
                modifier = Modifier.fillMaxWidth(),
            )
        }

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

        AuroraSeparator()
        val turns by vm.turns.collectAsStateWithLifecycle()
        if (turns.isNotEmpty()) {
            AuroraStatusIndicator(
                tone = AuroraStatusTone.Automating,
                label = "Follow-up · ${turns.size} prior turn${if (turns.size == 1) "" else "s"}",
                modifier = Modifier.padding(bottom = 4.dp),
            )
        }
        AuroraPromptInput(
            value = input,
            onValueChange = { input = it },
            onSend = {
                vm.ask(input)
                input = ""
            },
            placeholder = "Ask anything about your indexed knowledge…",
            loading = uiState is AskUiState.Loading || uiState is AskUiState.Streaming,
            modifier = Modifier.fillMaxWidth(),
        )
    }
    FabLauncher(
        onOpSubmit = { op, input -> vm.submitFabOp(op, input) },
    )
    } // end Box
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
            Text(
                entry.query,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                fmt.format(Date(entry.askedAt)),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
