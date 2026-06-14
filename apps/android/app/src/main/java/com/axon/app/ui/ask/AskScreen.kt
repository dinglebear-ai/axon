package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.fab.FabLauncher
import com.axon.app.ui.jobs.rememberRevealState
import com.axon.app.ui.jobs.revealOnce
import com.axon.app.ui.theme.AxonTheme
import tv.tootie.aurora.components.AuroraThinking

@Composable
fun AskScreen(
    onOpenDocument: (String) -> Unit = {},
    onFabOverlayVisibleChange: (Boolean) -> Unit = {},
    vm: AskViewModel = viewModel(),
) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()
    val chatItems by vm.chatItems.collectAsStateWithLifecycle()
    val history by vm.history.collectAsStateWithLifecycle()
    val historyReady by vm.historyReady.collectAsStateWithLifecycle()
    val mode by vm.mode.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }
    val listState = rememberLazyListState()
    val reveal = rememberRevealState()
    val historyPreview = remember(chatItems, history) {
        if (chatItems.isEmpty()) history.take(4).asReversed() else emptyList()
    }

    LaunchedEffect(chatItems.size) {
        if (chatItems.isNotEmpty()) listState.animateScrollToItem(chatItems.size - 1)
    }

    Box(modifier = Modifier.fillMaxSize().background(AxonTheme.colors.pageBg)) {
        Column(modifier = Modifier.fillMaxSize()) {
            Box(modifier = Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                AskModeSwitch(
                    mode = mode,
                    onModeChange = vm::setMode,
                    modifier = Modifier
                        .fillMaxWidth(0.50f)
                        .widthIn(max = 196.dp)
                        .padding(top = 8.dp),
                )
            }
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentAlignment = Alignment.TopCenter,
            ) {
                LazyColumn(
                    state = listState,
                    modifier = Modifier
                        .fillMaxWidth(0.90f)
                        .widthIn(max = 366.dp),
                    contentPadding = PaddingValues(start = 8.dp, top = 12.dp, end = 8.dp, bottom = 132.dp),
                    verticalArrangement = Arrangement.spacedBy(9.dp),
                ) {
                    when {
                        chatItems.isNotEmpty() -> {
                            itemsIndexed(
                                items = chatItems,
                                key = { index, item -> stableChatItemKey(index, item) },
                            ) { index, item ->
                                // New bubbles fade + slide in once; streaming
                                // updates reuse the stable key so they don't replay.
                                Box(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .animateItem()
                                        .revealOnce(reveal, stableChatItemKey(index, item), index, staggerMs = 0),
                                ) {
                                    ChatItemContent(item = item, onOpenDocument = onOpenDocument)
                                }
                            }
                        }
                        historyPreview.isNotEmpty() -> {
                            itemsIndexed(
                                items = historyPreview,
                                key = { _, item -> "history-${item.id}-${item.askedAt}" },
                            ) { index, item ->
                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .animateItem()
                                        .revealOnce(reveal, "history-${item.id}-${item.askedAt}", index, staggerMs = 0),
                                ) {
                                    UserBubble(item.query)
                                    Spacer(Modifier.height(4.dp))
                                    AxonBubble(item.answer, onOpenDocument = onOpenDocument)
                                }
                            }
                        }
                        !historyReady -> {
                            item {
                                Box(
                                    modifier = Modifier
                                        .fillParentMaxHeight()
                                        .fillMaxWidth()
                                        .padding(bottom = 72.dp),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    AuroraThinking(label = "Loading conversation…")
                                }
                            }
                        }
                        else -> {
                            item {
                                EmptyAskState(
                                    modifier = Modifier
                                        .fillParentMaxHeight()
                                        .fillMaxWidth(),
                                )
                            }
                        }
                    }
                }
            }
            Box(modifier = Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                AskPromptBar(
                    value = input,
                    onValueChange = { input = it },
                    loading = uiState is AskUiState.Loading || uiState is AskUiState.Streaming,
                    placeholder = "Ask a follow-up...",
                    modifier = Modifier
                        .fillMaxWidth(0.90f)
                        .widthIn(max = 366.dp)
                        .imePadding()
                        .navigationBarsPadding()
                        .padding(horizontal = 0.dp, vertical = 9.dp),
                    onSend = {
                        vm.ask(input)
                        input = ""
                    },
                )
            }
        }
        if (chatItems.isNotEmpty() || historyPreview.isNotEmpty()) {
            AuroraScrollThumb(
                listState = listState,
                modifier = Modifier
                    .align(Alignment.CenterEnd)
                    .padding(end = 3.dp, bottom = 88.dp),
            )
        }
        FabLauncher(
            onOpSubmit = { op, fabInput -> vm.submitFabOp(op, fabInput) },
            onOverlayVisibleChange = onFabOverlayVisibleChange,
        )
    }
}

@Composable
private fun ChatItemContent(item: ChatItem, onOpenDocument: (String) -> Unit) {
    when (item) {
        is ChatItem.UserMsg -> UserBubble(item.text)
        is ChatItem.AxonMsg -> AxonBubble(
            text = item.text,
            isStreaming = item.isStreaming,
            onOpenDocument = onOpenDocument,
        )
        is ChatItem.Activity -> ActivityRailRow(item)
        is ChatItem.ActionResult -> Box(
            modifier = Modifier.fillMaxWidth(),
            contentAlignment = Alignment.Center,
        ) {
            ActionResultCard(item)
        }
        is ChatItem.Injection -> Box(
            modifier = Modifier.fillMaxWidth(),
            contentAlignment = Alignment.Center,
        ) {
            InjectionCard(item)
        }
    }
}
