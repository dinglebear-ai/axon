package com.axon.app.ui.ask

import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.fab.FabLauncher
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.ui.theme.AxonTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import tv.tootie.aurora.components.AuroraThinking

@Composable
fun AskScreen(
    onOpenDocument: (String) -> Unit = {},
    onOpenJobs: () -> Unit = {},
    onFabOverlayVisibleChange: (Boolean) -> Unit = {},
    vm: AskViewModel = viewModel(),
) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()
    val chatItems by vm.chatItems.collectAsStateWithLifecycle()
    val history by vm.history.collectAsStateWithLifecycle()
    val historyReady by vm.historyReady.collectAsStateWithLifecycle()
    val mode by vm.mode.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }
    var attachments by remember { mutableStateOf<List<PromptAttachment>>(emptyList()) }
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val pickAttachment = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenMultipleDocuments(),
    ) { uris ->
        if (uris.isNotEmpty()) {
            scope.launch {
                val results = uris.map { uri -> withContext(Dispatchers.IO) { readPromptAttachment(context, uri) } }
                val ok = results.mapNotNull { it.getOrNull() }
                val failed = results.size - ok.size
                val before = attachments.size
                if (ok.isNotEmpty()) {
                    attachments = (attachments + ok).distinctBy { it.name }.take(MAX_ATTACHMENTS)
                }
                // Files successfully read but dropped by dedupe (same display name)
                // or the MAX_ATTACHMENTS cap — not counted in `failed`.
                val accepted = (attachments.size - before).coerceAtLeast(0)
                val skipped = ok.size - accepted

                // Build one combined Toast so the user isn't spammed with two.
                val readFailure = results.firstOrNull { it.isFailure }?.exceptionOrNull()?.message
                val failMsg = when {
                    failed == 1 -> readFailure ?: "1 file couldn't be attached"
                    failed > 1 -> buildString {
                        append("$failed files couldn't be attached")
                        if (readFailure != null) append(" — $readFailure")
                    }
                    else -> null
                }
                val skipMsg = if (skipped > 0) {
                    "Some files were skipped (duplicate name or $MAX_ATTACHMENTS-file limit)."
                } else {
                    null
                }
                val message = listOfNotNull(failMsg, skipMsg).joinToString("\n").ifBlank { null }
                if (message != null) {
                    Toast.makeText(context, message, Toast.LENGTH_LONG).show()
                }
            }
        }
    }
    val listState = rememberLazyListState()
    val reveal = rememberRevealState()
    val lastAxonIdx = chatItems.indexOfLast { it is ChatItem.AxonMsg }
    // Length of the in-flight streamed answer; drives the follow-to-bottom effect below.
    val streamingLen = (chatItems.lastOrNull() as? ChatItem.AxonMsg)
        ?.takeIf { it.isStreaming }?.text?.length ?: 0
    // Sticky "parked at the bottom" flag: disengages only on a real upward drag,
    // re-engages on reaching the end. Streaming growth alone never unsticks it, so
    // the answer keeps following — but a user who scrolls up is left alone.
    var followBottom by remember { mutableStateOf(true) }
    LaunchedEffect(listState) {
        var prevIndex = listState.firstVisibleItemIndex
        var prevOffset = listState.firstVisibleItemScrollOffset
        snapshotFlow { listState.firstVisibleItemIndex to listState.firstVisibleItemScrollOffset }
            .collect { (idx, off) ->
                if (idx < prevIndex || (idx == prevIndex && off < prevOffset)) followBottom = false
                if (!listState.canScrollForward) followBottom = true
                prevIndex = idx
                prevOffset = off
            }
    }
    val askSuggestions = remember {
        listOf(
            "What can Axon help me do?",
            "Help me choose between Ask and Chat mode",
            "How should I crawl and index a docs site?",
        )
    }
    fun submitStarterPrompt(prompt: String) {
        vm.setMode(ConversationMode.Chat)
        vm.ask(prompt)
    }
    fun copyMessage(value: String) {
        context.getSystemService(android.content.ClipboardManager::class.java)
            ?.setPrimaryClip(android.content.ClipData.newPlainText("Axon message", value))
        Toast.makeText(context, "Copied", Toast.LENGTH_SHORT).show()
    }
    val historyPreview = remember(chatItems, history) {
        if (chatItems.isEmpty()) history.take(4).asReversed() else emptyList()
    }

    LaunchedEffect(chatItems.size) {
        if (chatItems.isNotEmpty()) {
            listState.animateScrollToItem(chatItems.lastIndex, Int.MAX_VALUE / 2)
        }
    }
    // Follow the streamed answer as it grows, pinned to the bottom unless the
    // user has scrolled away to read earlier turns.
    LaunchedEffect(streamingLen) {
        if (streamingLen > 0 && followBottom && chatItems.isNotEmpty()) {
            listState.scrollToItem(chatItems.lastIndex, Int.MAX_VALUE / 2)
        }
    }

    Box(modifier = Modifier.fillMaxSize().background(AxonTheme.colors.pageBg)) {
        Column(modifier = Modifier.fillMaxSize()) {
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentAlignment = Alignment.TopCenter,
            ) {
                LazyColumn(
                    state = listState,
                    modifier = Modifier
                        .fillMaxWidth()
                        .widthIn(max = 460.dp),
                    contentPadding = PaddingValues(start = 6.dp, top = 12.dp, end = 6.dp, bottom = 176.dp),
                ) {
                    when {
                        chatItems.isNotEmpty() -> {
                            itemsIndexed(
                                items = chatItems,
                                key = { index, item -> stableChatItemKey(index, item) },
                            ) { index, item ->
                                // Group consecutive same-sender messages: a wider gap
                                // when the sender flips, a tight gap within a run.
                                val prev = chatItems.getOrNull(index - 1)
                                val isSenderStart = prev == null || chatSenderSide(prev) != chatSenderSide(item)
                                val showAvatar = when (item) {
                                    is ChatItem.UserMsg -> prev !is ChatItem.UserMsg
                                    is ChatItem.AxonMsg -> prev !is ChatItem.AxonMsg
                                    else -> true
                                }
                                val topGap = if (index == 0) 0.dp else if (isSenderStart) 14.dp else 6.dp
                                // New bubbles fade + slide in once; streaming
                                // updates reuse the stable key so they don't replay.
                                Box(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(top = topGap)
                                        .animateItem()
                                        .revealOnce(reveal, stableChatItemKey(index, item), index, staggerMs = 0),
                                ) {
                                    ChatItemContent(
                                        item = item,
                                        onOpenDocument = onOpenDocument,
                                        onOpenJobs = onOpenJobs,
                                        isLastAxon = index == lastAxonIdx,
                                        showAvatar = showAvatar,
                                        onCopy = ::copyMessage,
                                        onEdit = { input = it },
                                        onRegenerate = { vm.regenerateLast() },
                                    )
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
                                        .padding(top = if (index == 0) 0.dp else 11.dp)
                                        .animateItem()
                                        .revealOnce(reveal, "history-${item.id}-${item.askedAt}", index, staggerMs = 0),
                                ) {
                                    UserBubble(
                                        item.query,
                                        timestamp = item.askedAt,
                                        onEdit = { input = item.query },
                                        onCopy = { copyMessage(item.query) },
                                    )
                                    Spacer(Modifier.height(4.dp))
                                    AxonBubble(
                                        item.answer,
                                        onOpenDocument = onOpenDocument,
                                        timestamp = item.askedAt,
                                        onCopy = { copyMessage(item.answer) },
                                    )
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
                                    suggestions = askSuggestions,
                                    onSuggestion = ::submitStarterPrompt,
                                )
                            }
                        }
                    }
                }
            }
            Column(
                modifier = Modifier.fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                ModeExplanationPill(
                    mode = mode,
                    modifier = Modifier
                        .fillMaxWidth()
                        .widthIn(max = 460.dp)
                        .padding(horizontal = 12.dp),
                )
                AskPromptBar(
                    value = input,
                    onValueChange = { input = it },
                    loading = uiState is AskUiState.Loading || uiState is AskUiState.Streaming,
                    placeholder = when {
                        mode == ConversationMode.Chat -> "Chat with Axon…"
                        chatItems.isEmpty() && historyPreview.isEmpty() -> "Ask indexed docs…"
                        else -> "Ask a follow-up…"
                    },
                    mode = mode,
                    onModeChange = vm::setMode,
                    attachments = attachments,
                    onAttachClick = { pickAttachment.launch(arrayOf("*/*")) },
                    onRemoveAttachment = { idx -> attachments = attachments.filterIndexed { i, _ -> i != idx } },
                    onStop = { vm.stopGeneration() },
                    modifier = Modifier
                        .fillMaxWidth()
                        .widthIn(max = 460.dp)
                        .imePadding()
                        .navigationBarsPadding()
                        .padding(horizontal = 6.dp, vertical = 12.dp),
                    onSend = {
                        vm.ask(input, attachment = combinedAttachmentText(attachments))
                        input = ""
                        attachments = emptyList()
                    },
                )
            }
        }
        if (chatItems.isNotEmpty() || historyPreview.isNotEmpty()) {
            AuroraScrollThumb(
                listState = listState,
                modifier = Modifier
                    .align(Alignment.CenterEnd)
                    .padding(end = 3.dp, bottom = 116.dp),
            )
        }
        val lastListIndex = if (chatItems.isNotEmpty()) chatItems.lastIndex else historyPreview.lastIndex
        JumpToLatest(
            visible = !followBottom && listState.canScrollForward && lastListIndex >= 0,
            onClick = {
                followBottom = true
                scope.launch { listState.animateScrollToItem(lastListIndex, Int.MAX_VALUE / 2) }
            },
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = 120.dp),
        )
        FabLauncher(
            onOpSubmit = { op, fabInput -> vm.submitFabOp(op, fabInput) },
            onOverlayVisibleChange = onFabOverlayVisibleChange,
        )
    }
}

@Composable
private fun ChatItemContent(
    item: ChatItem,
    onOpenDocument: (String) -> Unit,
    onOpenJobs: () -> Unit,
    isLastAxon: Boolean,
    showAvatar: Boolean,
    onCopy: (String) -> Unit,
    onEdit: (String) -> Unit,
    onRegenerate: () -> Unit,
) {
    when (item) {
        is ChatItem.UserMsg -> UserBubble(
            text = item.text,
            timestamp = item.timestamp,
            showAvatar = showAvatar,
            onEdit = { onEdit(item.text) },
            onCopy = { onCopy(item.text) },
        )
        is ChatItem.AxonMsg -> AxonBubble(
            text = item.text,
            isStreaming = item.isStreaming,
            onOpenDocument = onOpenDocument,
            timestamp = item.timestamp,
            showAvatar = showAvatar,
            onCopy = if (item.text.isNotBlank()) { { onCopy(item.text) } } else null,
            onRegenerate = if (isLastAxon && !item.isStreaming) onRegenerate else null,
        )
        is ChatItem.Activity -> ActivityRailRow(item)
        is ChatItem.ActionResult -> Box(
            modifier = Modifier.fillMaxWidth(),
            contentAlignment = Alignment.Center,
        ) {
            ActionResultCard(item, onOpenJobs = onOpenJobs)
        }
        is ChatItem.Injection -> Box(
            modifier = Modifier.fillMaxWidth(),
            contentAlignment = Alignment.Center,
        ) {
            InjectionCard(item, onOpenJobs = onOpenJobs)
        }
    }
}

/** 0 = user side, 1 = assistant side (answers, tool activity, op results). */
internal fun chatSenderSide(item: ChatItem): Int = if (item is ChatItem.UserMsg) 0 else 1

/** Max files attachable to a single prompt. */
private const val MAX_ATTACHMENTS = 6

/** Concatenate attached files into one labeled block inlined into the question (null when none). */
internal fun combinedAttachmentText(attachments: List<PromptAttachment>): String? =
    if (attachments.isEmpty()) {
        null
    } else {
        attachments.joinToString("\n\n") { "=== ${it.name} ===\n${it.content}" }
    }
