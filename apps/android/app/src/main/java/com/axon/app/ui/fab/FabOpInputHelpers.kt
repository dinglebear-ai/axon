package com.axon.app.ui.fab

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoFixHigh
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.ToneTrio
import com.axon.app.ui.theme.tint

internal fun normalizeFabInput(
    op: FabOp,
    input: String,
): String {
    val trimmed = input.trim()
    if (trimmed.isBlank()) return ""
    return if (op.expectsUrl() && !trimmed.contains("://")) "https://$trimmed" else trimmed
}

internal fun fabInputCanSubmit(
    op: FabOp,
    input: String,
    broadActionConfirmed: Boolean,
): Boolean {
    val normalized = normalizeFabInput(op, input)
    return normalized.isNotBlank() && (op.broadActionConfirmationLabel() == null || broadActionConfirmed)
}

internal fun FabOp.expectsUrl(): Boolean =
    when (this) {
        FabOp.Scrape,
        FabOp.Extract,
        FabOp.Map,
        FabOp.Retrieve,
        FabOp.Summarize,
        FabOp.SourceSite,
        -> true

        FabOp.Research,
        FabOp.Embed,
        FabOp.Query,
        FabOp.Search,
        FabOp.Source,
        -> false
    }

internal fun FabOp.shortDescription(): String =
    when (this) {
        FabOp.Scrape -> "Fetch one page → markdown"
        FabOp.Research -> "Search + synthesize"
        FabOp.Extract -> "Structured extraction"
        FabOp.Embed -> "Index content"
        FabOp.Query -> "Semantic vector search"
        FabOp.Search -> "Web search + index"
        FabOp.Map -> "Discover site URLs"
        FabOp.Retrieve -> "Fetch indexed chunks"
        FabOp.Summarize -> "Summarize a document"
        FabOp.SourceSite -> "Index a multi-page site"
        FabOp.Source -> "Import repo, reddit, or media"
    }

internal fun FabOp.broadActionConfirmationLabel(): String? =
    when (this) {
        FabOp.SourceSite -> "Run with current site-source defaults/options"
        FabOp.Source -> "Run with current source defaults/options"
        else -> null
    }

internal fun FabOp.inputPlaceholder(): String =
    when (this) {
        FabOp.Scrape -> "Page URL, e.g. https://example.com/docs"
        FabOp.Research -> "Research question or topic"
        FabOp.Extract -> "Page URL to extract structured data from"
        FabOp.Embed -> "URL, server path, or text to index"
        FabOp.Query -> "Question to search indexed content"
        FabOp.Search -> "Web search query"
        FabOp.Map -> "Site URL to discover"
        FabOp.Retrieve -> "Indexed URL to retrieve"
        FabOp.Summarize -> "Page URL to summarize"
        FabOp.SourceSite -> "Docs/site URL to index"
        FabOp.Source -> "GitHub repo, feed, reddit, or YouTube URL"
    }

internal fun FabOp.inputExamples(): List<String> =
    when (this) {
        FabOp.Scrape -> listOf("https://example.com/docs", "axon.tootie.tv")
        FabOp.Research -> listOf("latest Android edge-to-edge guidance", "how to structure a RAG eval")
        FabOp.Extract -> listOf("https://example.com/product", "https://github.com/jmagar/axon")
        FabOp.Embed -> listOf("https://example.com/docs", "/home/jmagar/workspace/axon/docs")
        FabOp.Query -> listOf("watch scheduler lease handling", "hybrid search recall knobs")
        FabOp.Search -> listOf("Qwen3 embedding model dimensions", "Spider.rs crawl examples")
        FabOp.Map -> listOf("https://example.com", "https://docs.rs")
        FabOp.Retrieve -> listOf("https://example.com/docs/intro", "https://docs.rs/spider")
        FabOp.Summarize -> listOf("https://example.com/blog/post", "https://docs.rs/spider/latest/spider/")
        FabOp.SourceSite -> listOf("https://example.com/docs", "https://docs.rs/spider")
        FabOp.Source -> listOf("github.com/jmagar/axon", "https://www.youtube.com/watch?v=dQw4w9WgXcQ")
    }

@Composable
internal fun ExampleInputChip(
    text: String,
    tone: ToneTrio,
    onClick: () -> Unit,
) {
    val colors = AxonTheme.colors
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(9.dp))
                .background(colors.control.copy(alpha = 0.42f), RoundedCornerShape(9.dp))
                .border(1.dp, colors.tint(tone.base, 14, colors.control), RoundedCornerShape(9.dp))
                .semantics(mergeDescendants = true) {
                    contentDescription = "Use example $text"
                    role = Role.Button
                }.pressScale(role = Role.Button, onClick = onClick)
                .padding(horizontal = 9.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Icon(
            Icons.Rounded.AutoFixHigh,
            contentDescription = null,
            tint = tone.fg.copy(alpha = 0.72f),
            modifier = Modifier.size(12.dp),
        )
        Text(
            text,
            color = colors.textMuted.copy(alpha = 0.88f),
            fontSize = 10.6.sp,
            lineHeight = 14.sp,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}
