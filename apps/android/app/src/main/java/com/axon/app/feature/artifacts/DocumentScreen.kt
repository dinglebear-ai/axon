package com.axon.app.feature.artifacts

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.Link
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.RetrieveResultUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf

@Composable
fun DocumentScreen(
    url: String,
    vm: DocumentViewModel = viewModel(),
) {
    LaunchedEffect(url) { vm.load(url) }
    val state by vm.uiState.collectAsStateWithLifecycle()

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(AxonTheme.colors.pageBg),
        contentAlignment = Alignment.TopCenter,
    ) {
        when (val s = state) {
            is DocumentUiState.Loading -> LoadingContent(
                label = "Fetching document...",
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp),
            )

            is DocumentUiState.Error -> ErrorContent(
                message = s.message,
                onRetry = { vm.retry(url) },
                modifier = Modifier.padding(16.dp),
            )

            is DocumentUiState.Success -> DocumentViewer(result = s.result)
        }
    }
}

@Composable
private fun DocumentViewer(result: RetrieveResultUi) {
    val blocks = remember(result.content) { markdownBlocks(result.content) }
    val title = remember(result) { documentTitle(result, blocks) }
    val visibleWarnings = remember(result.truncated, result.warnings) {
        visibleDocumentWarnings(result.truncated, result.warnings)
    }
    LazyColumn(
        modifier = Modifier
            .fillMaxWidth(0.88f)
            .widthIn(max = 376.dp)
            .padding(top = 10.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        item {
            RetrieveResultHeader(result = result, title = title)
        }

        if (visibleWarnings.isNotEmpty()) {
            item {
                WarningPanel(
                    text = visibleWarnings.joinToString(" "),
                )
            }
        }

        if (blocks.isEmpty()) {
            item {
                EmptyContent(
                    title = "Document is empty",
                    description = "The server returned a stored document with no readable body.",
                    icon = Icons.Rounded.Description,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        } else {
            items(blocks, key = { it.key }) { block ->
                DocumentBlockView(block)
            }
            if (result.truncated) {
                item {
                    LoadMoreChunksHint(result)
                }
            }
        }
    }
}

@Composable
private fun RetrieveResultHeader(result: RetrieveResultUi, title: String) {
    val colors = AxonTheme.colors
    val tone = colors.toneOf(AxonTone.Cyan)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 2.dp, vertical = 2.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(Icons.Rounded.Description, contentDescription = null, tint = colors.textMuted.copy(alpha = 0.82f), modifier = Modifier.size(15.dp))
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                Text(
                    "Document",
                    color = colors.textPrimary,
                    fontSize = 12.8.sp,
                    lineHeight = 16.2.sp,
                    fontWeight = FontWeight.ExtraBold,
                    fontFamily = AxonTheme.fonts.display,
                )
                Text(
                    "GET /v1/retrieve",
                    color = colors.textMuted,
                    fontSize = 9.6.sp,
                    lineHeight = 12.2.sp,
                    fontFamily = AxonTheme.fonts.mono,
                )
            }
            StatusPill("200 OK")
        }

        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            MetaPill(shortUrl(result.matchedUrl ?: result.requestedUrl), icon = true)
            MetaPill("${result.chunkCount} ${if (result.chunkCount == 1) "chunk" else "chunks"}", accent = true)
            result.tokenEstimate?.let { MetaPill("~${it.formatCount()} tokens") }
            result.refreshStatus?.takeIf { it.isNotBlank() }?.let { MetaPill(it, success = true) }
            if (result.truncated) MetaPill("truncated", warn = true)
        }

        Text(
            title,
            color = colors.textPrimary,
            fontSize = 14.sp,
            lineHeight = 18.sp,
            fontWeight = FontWeight.ExtraBold,
            fontFamily = AxonTheme.fonts.display,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun LoadMoreChunksHint(result: RetrieveResultUi) {
    val colors = AxonTheme.colors
    val tone = colors.toneOf(AxonTone.Cyan)
    val cursor = result.nextCursor?.take(6)?.let { " · cursor $it..." }.orEmpty()
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(11.dp))
            .background(colors.tint(tone.base, 12, colors.control), RoundedCornerShape(11.dp))
            .border(1.dp, colors.tint(tone.base, 30, colors.control), RoundedCornerShape(11.dp))
            .padding(horizontal = 13.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Text(
            "Load more chunks$cursor",
            color = tone.fg,
            fontSize = 13.8.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun DocumentBlockView(block: DocumentBlock) {
    val colors = AxonTheme.colors
    when (block.kind) {
        DocumentBlockKind.Heading -> Text(
            block.text,
            color = colors.textPrimary,
            fontSize = 13.2.sp,
            lineHeight = 17.sp,
            fontWeight = FontWeight.ExtraBold,
            fontFamily = AxonTheme.fonts.display,
            modifier = Modifier.fillMaxWidth().padding(top = 2.dp),
        )
        DocumentBlockKind.Body -> SelectionContainer {
            Text(
                block.text,
                color = colors.textPrimary.copy(alpha = 0.92f),
                fontSize = 10.7.sp,
                lineHeight = 15.7.sp,
                fontFamily = AxonTheme.fonts.body,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        DocumentBlockKind.Code -> SelectionContainer {
            Text(
                block.text,
                color = colors.textMuted,
                fontSize = 10.4.sp,
                lineHeight = 15.2.sp,
                fontFamily = AxonTheme.fonts.mono,
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(9.dp))
                    .background(colors.panelMedium.copy(alpha = 0.72f), RoundedCornerShape(9.dp))
                    .border(1.dp, colors.borderDefault.copy(alpha = 0.76f), RoundedCornerShape(9.dp))
                    .padding(8.dp),
            )
        }
    }
}

@Composable
private fun WarningPanel(text: String) {
    val colors = AxonTheme.colors
    Text(
        text,
        color = colors.warn,
        fontSize = 9.6.sp,
        lineHeight = 13.4.sp,
        fontFamily = AxonTheme.fonts.body,
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(9.dp))
            .background(colors.tint(colors.warn, 7, colors.control), RoundedCornerShape(9.dp))
            .border(1.dp, colors.tint(colors.warn, 22, colors.control), RoundedCornerShape(9.dp))
            .padding(horizontal = 7.dp, vertical = 6.dp),
    )
}

@Composable
private fun StatusPill(status: String) {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(colors.success, 12, colors.control))
            .border(1.dp, colors.tint(colors.success, 28, colors.control), RoundedCornerShape(999.dp))
            .padding(horizontal = 6.dp, vertical = 3.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Icon(Icons.Rounded.CheckCircle, contentDescription = null, tint = colors.success, modifier = Modifier.size(10.dp))
        Text(status, color = colors.success, fontSize = 9.sp, fontWeight = FontWeight.SemiBold, fontFamily = AxonTheme.fonts.mono)
    }
}

@Composable
private fun MetaPill(text: String, accent: Boolean = false, warn: Boolean = false, success: Boolean = false, icon: Boolean = false) {
    val colors = AxonTheme.colors
    val tone = colors.toneOf(AxonTone.Cyan)
    val base = when {
        warn -> colors.warn
        success -> colors.success
        accent -> tone.base
        else -> colors.borderStrong
    }
    Row(
        modifier = Modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(base, 10, colors.control))
            .border(1.dp, colors.tint(base, 25, colors.control), RoundedCornerShape(999.dp))
            .padding(horizontal = 6.dp, vertical = 3.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        if (icon) Icon(Icons.Rounded.Link, contentDescription = null, tint = tone.fg, modifier = Modifier.size(9.dp))
        Text(
            text,
            color = when {
                warn -> colors.warn
                success -> colors.success
                accent -> tone.fg
                else -> colors.textMuted
            },
            fontSize = 8.9.sp,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

