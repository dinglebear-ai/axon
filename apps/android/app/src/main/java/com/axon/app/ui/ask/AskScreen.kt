package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.fab.FabLauncher
import com.axon.app.ui.theme.AxonTheme
import tv.tootie.aurora.components.AuroraPromptInput

@Composable
fun AskScreen(
    onOpenDocument: (String) -> Unit = {},
    onFabOverlayVisibleChange: (Boolean) -> Unit = {},
    vm: AskViewModel = viewModel(),
) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()
    val chatItems by vm.chatItems.collectAsStateWithLifecycle()
    val turns by vm.turns.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }
    val listState = rememberLazyListState()

    LaunchedEffect(chatItems.size) {
        if (chatItems.isNotEmpty()) listState.animateScrollToItem(chatItems.size - 1)
    }

    Box(modifier = Modifier.fillMaxSize().background(AxonTheme.colors.pageBg)) {
        Column(modifier = Modifier.fillMaxSize()) {
            LazyColumn(
                state = listState,
                modifier = Modifier.weight(1f).fillMaxWidth(),
                contentPadding = PaddingValues(start = 12.dp, top = 10.dp, end = 12.dp, bottom = 10.dp),
                verticalArrangement = Arrangement.spacedBy(9.dp),
            ) {
                if (chatItems.isEmpty()) {
                    item {
                        EmptyAskState()
                    }
                } else {
                    itemsIndexed(
                        items = chatItems,
                        key = { index, item -> stableChatItemKey(index, item) },
                    ) { _, item ->
                        when (item) {
                            is ChatItem.UserMsg -> UserBubble(item.text)
                            is ChatItem.AxonMsg -> AxonBubble(item.text, item.isStreaming)
                            is ChatItem.Injection -> InjectionCard(item.op, item.target, item.jobId, item.pageCount, item.chunkCount)
                        }
                    }
                }
            }
            AuroraPromptInput(
                value = input,
                onValueChange = { input = it },
                loading = uiState is AskUiState.Loading || uiState is AskUiState.Streaming,
                placeholder = "Ask a follow-up...",
                modifier = Modifier
                    .fillMaxWidth()
                    .imePadding()
                    .navigationBarsPadding()
                    .padding(horizontal = 12.dp, vertical = 9.dp),
                leadingContent = if (turns.isNotEmpty()) {
                    {
                        Text(
                            "Follow-up context: ${turns.size} prior turn${if (turns.size == 1) "" else "s"}",
                            color = AxonTheme.colors.textMuted,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.SemiBold,
                            fontFamily = AxonTheme.fonts.body,
                            modifier = Modifier.padding(bottom = 7.dp),
                        )
                    }
                } else null,
                onSend = {
                    vm.ask(input)
                    input = ""
                },
            )
        }
        AuroraScrollThumb(
            modifier = Modifier
                .align(Alignment.CenterEnd)
                .padding(end = 3.dp, bottom = 88.dp),
        )
        FabLauncher(
            onOpSubmit = { op, fabInput -> vm.submitFabOp(op, fabInput) },
            onOverlayVisibleChange = onFabOverlayVisibleChange,
        )
    }

    @Suppress("UNUSED_EXPRESSION")
    onOpenDocument
}

@Composable
private fun EmptyAskState() {
    val colors = AxonTheme.colors
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 96.dp, start = 18.dp, end = 18.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("Ask Axon", color = colors.textPrimary, fontSize = 17.sp, fontWeight = FontWeight.ExtraBold, fontFamily = AxonTheme.fonts.display)
        Text("Run a live ask or open the operation ring to call Axon services.", color = colors.textMuted, fontSize = 12.sp, lineHeight = 17.sp, fontFamily = AxonTheme.fonts.body)
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
