package com.axon.app.ui.ask

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
    val chatItems by vm.chatItems.collectAsStateWithLifecycle()
    val turns by vm.turns.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }
    val listState = rememberLazyListState()

    // Auto-scroll to bottom when new items arrive
    LaunchedEffect(chatItems.size) {
        if (chatItems.isNotEmpty()) listState.animateScrollToItem(chatItems.size - 1)
    }

    Box(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            // Chat message list or empty/history state
            if (chatItems.isNotEmpty()) {
                LazyColumn(
                    state = listState,
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxWidth(),
                    contentPadding = PaddingValues(vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    items(chatItems, key = { it.hashCode() }) { item ->
                        when (item) {
                            is ChatItem.UserMsg   -> UserBubble(item.text)
                            is ChatItem.AxonMsg   -> AxonBubble(item.text, item.isStreaming)
                            is ChatItem.Injection -> InjectionCard(item.op, item.target, item.pageCount, item.chunkCount)
                        }
                    }
                }
            } else {
                Spacer(Modifier.weight(1f))
                AnimatedVisibility(visible = history.isEmpty()) {
                    EmptyContent(
                        title = "Ask anything",
                        description = "Ask anything about your indexed knowledge",
                        icon = Icons.Outlined.AutoAwesome,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                AnimatedVisibility(visible = history.isNotEmpty()) {
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
            }

            AuroraSeparator()
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
            onOpSubmit = { op, fabInput -> vm.submitFabOp(op, fabInput) },
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
