package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.rounded.AttachFile
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.fab.FabLauncher
import com.axon.app.ui.nav.AxonMarkGlyph
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import tv.tootie.aurora.components.AuroraSpinner
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
                            ) { _, item ->
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
                        }
                        historyPreview.isNotEmpty() -> {
                            itemsIndexed(
                                items = historyPreview,
                                key = { _, item -> "history-${item.id}-${item.askedAt}" },
                            ) { _, item ->
                                UserBubble(item.query)
                                Spacer(Modifier.height(4.dp))
                                AxonBubble(item.answer, onOpenDocument = onOpenDocument)
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
private fun AskModeSwitch(
    mode: ConversationMode,
    onModeChange: (ConversationMode) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(999.dp)
    Row(
        modifier = modifier
            .height(30.dp)
            .clip(shape)
            .background(colors.control.copy(alpha = 0.58f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.56f), shape)
            .padding(3.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        ConversationMode.entries.forEach { item ->
            val selected = item == mode
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .height(24.dp)
                    .clip(RoundedCornerShape(999.dp))
                    .background(
                        if (selected) colors.tint(colors.accentPrimary, 12, colors.control) else Color.Transparent,
                        RoundedCornerShape(999.dp),
                    )
                    .border(
                        1.dp,
                        if (selected) colors.tint(colors.accentPrimary, 28, colors.control) else Color.Transparent,
                        RoundedCornerShape(999.dp),
                    )
                    .clickable(enabled = !selected) { onModeChange(item) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    item.label,
                    color = if (selected) colors.accentStrong else colors.textMuted.copy(alpha = 0.78f),
                    fontSize = 10.6.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = AxonTheme.fonts.body,
                )
            }
        }
    }
}

@Composable
private fun EmptyAskState(modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    Box(
        modifier = modifier.padding(bottom = 72.dp),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier.widthIn(max = 292.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(13.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(58.dp)
                    .clip(RoundedCornerShape(18.dp))
                    .background(colors.tint(colors.accentPrimary, 13, colors.control))
                    .border(1.dp, colors.tint(colors.accentPrimary, 30, colors.control), RoundedCornerShape(18.dp)),
                contentAlignment = Alignment.Center,
            ) {
                AxonMarkGlyph(Modifier.size(34.dp))
            }
            Text(
                "No active conversation",
                color = colors.textPrimary,
                fontSize = 16.sp,
                fontFamily = AxonTheme.fonts.display,
            )
        }
    }
}

@Composable
private fun AskPromptBar(
    value: String,
    onValueChange: (String) -> Unit,
    onSend: () -> Unit,
    loading: Boolean,
    placeholder: String,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val canSend = value.isNotBlank() && !loading
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(13.dp)
    val borderColor = colors.tint(colors.accentPrimary, if (focused) 20 else 6, colors.pageBg)

    fun triggerSend() {
        if (canSend) onSend()
    }

    Row(
        modifier = modifier
            .height(48.dp)
            .clip(shape)
            .background(colors.panelMedium.copy(alpha = if (focused) 0.16f else 0.10f), shape)
            .border(
                width = 1.dp,
                color = borderColor,
                shape = shape,
            )
            .padding(start = 9.dp, top = 4.dp, end = 6.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(
            modifier = Modifier
                .size(34.dp)
                .clip(RoundedCornerShape(9.dp))
                .clickable(enabled = false) {},
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Rounded.AttachFile,
                contentDescription = "Attach file",
                tint = colors.textMuted.copy(alpha = 0.72f),
                modifier = Modifier.size(17.dp),
            )
        }
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            enabled = !loading,
            singleLine = true,
            textStyle = TextStyle(
                color = colors.textPrimary,
                fontSize = 14.2.sp,
                fontFamily = AxonTheme.fonts.body,
            ),
            cursorBrush = SolidColor(colors.accentStrong),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = { triggerSend() }),
            modifier = Modifier
                .weight(1f)
                .onFocusChanged { focused = it.isFocused },
            decorationBox = { inner ->
                Box {
                    if (value.isEmpty()) {
                        Text(
                            placeholder,
                            color = colors.textMuted.copy(alpha = 0.72f),
                            fontSize = 14.2.sp,
                            fontFamily = AxonTheme.fonts.body,
                        )
                    }
                    inner()
                }
            },
        )
        Box(
            modifier = Modifier
                .size(36.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(if (canSend) colors.tint(colors.accentPrimary, 10, colors.control) else colors.control.copy(alpha = 0.34f), RoundedCornerShape(10.dp))
                .border(1.dp, if (canSend) colors.tint(colors.accentPrimary, 24, colors.control) else colors.borderDefault.copy(alpha = 0.42f), RoundedCornerShape(10.dp))
                .clickable(enabled = canSend, onClick = ::triggerSend),
            contentAlignment = Alignment.Center,
        ) {
            if (loading) {
                AuroraSpinner(contentDescription = "Sending", size = 15.dp)
            } else {
                Icon(
                    Icons.AutoMirrored.Filled.Send,
                    contentDescription = "Send message",
                    tint = if (canSend) colors.accentStrong.copy(alpha = 0.9f) else colors.textMuted.copy(alpha = 0.72f),
                    modifier = Modifier.size(17.dp),
                )
            }
        }
    }
}

@Composable
private fun AuroraScrollThumb(modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    Column(
        modifier = modifier
            .width(4.dp)
            .height(128.dp)
            .clip(RoundedCornerShape(999.dp))
            .background(Color.Transparent),
        verticalArrangement = Arrangement.Bottom,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            modifier = Modifier
                .width(4.dp)
                .height(45.dp)
                .clip(RoundedCornerShape(999.dp))
                .background(colors.borderStrong.copy(alpha = 0.72f)),
        )
    }
}
