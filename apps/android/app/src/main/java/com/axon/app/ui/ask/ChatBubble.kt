package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.nav.AxonMarkGlyph
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf
import tv.tootie.aurora.components.AuroraThinking
import java.net.URLDecoder
import java.nio.charset.StandardCharsets

@Composable
fun ActivityRailRow(item: ChatItem.Activity, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(start = 31.dp, top = 2.dp, bottom = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .weight(1f)
                .padding(start = 12.dp, top = 5.dp, bottom = 5.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Box(
                modifier = Modifier
                    .width(2.dp)
                    .height(22.dp)
                    .background(colors.borderDefault, RoundedCornerShape(999.dp)),
            )
            Icon(Icons.Rounded.Storage, contentDescription = null, tint = colors.accentStrong, modifier = Modifier.size(12.dp))
            Text(
                item.name,
                color = colors.textPrimary,
                fontSize = 10.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
            )
            Text(
                "(${item.arg})",
                color = colors.textMuted,
                fontSize = 10.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(5.dp),
                modifier = Modifier.padding(end = 2.dp),
            ) {
                if (item.done) {
                    Icon(Icons.Rounded.Check, contentDescription = null, tint = colors.success, modifier = Modifier.size(11.dp))
                }
                Text(
                    item.result,
                    color = if (item.done) colors.success else colors.textMuted,
                    fontSize = 10.sp,
                    fontFamily = AxonTheme.fonts.mono,
                    maxLines = 1,
                )
            }
        }
    }
}

@Composable
fun UserBubble(text: String, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    val displayText = remember(text) { displayUserText(text) }
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.End,
        verticalAlignment = Alignment.Top,
    ) {
        Row(
            modifier = Modifier.widthIn(max = 286.dp),
            horizontalArrangement = Arrangement.spacedBy(7.dp),
            verticalAlignment = Alignment.Top,
        ) {
            Column(
                modifier = Modifier.weight(1f, fill = false),
                horizontalAlignment = Alignment.End,
            ) {
                Text(
                    text = displayText,
                    color = colors.textPrimary,
                    fontSize = 13.6.sp,
                    lineHeight = 19.6.sp,
                    fontFamily = AxonTheme.fonts.body,
                    modifier = Modifier
                        .clip(RoundedCornerShape(topStart = 13.dp, topEnd = 4.dp, bottomStart = 13.dp, bottomEnd = 13.dp))
                        .background(colors.control.copy(alpha = 0.62f))
                        .border(
                            1.dp,
                            colors.borderDefault.copy(alpha = 0.72f),
                            RoundedCornerShape(topStart = 13.dp, topEnd = 4.dp, bottomStart = 13.dp, bottomEnd = 13.dp),
                        )
                        .padding(horizontal = 11.dp, vertical = 7.dp),
                )
            }
            UserInitials()
        }
    }
}

internal fun displayUserText(text: String): String {
    if (!text.contains('%')) return text
    return runCatching { URLDecoder.decode(text, StandardCharsets.UTF_8.name()) }
        .getOrDefault(text)
}

@Composable
fun AxonBubble(
    text: String,
    isStreaming: Boolean = false,
    modifier: Modifier = Modifier,
    onOpenDocument: (String) -> Unit = {},
) {
    val citations = rememberCitations(text)
    val displayText = remember(text) { humanizeJsonFragmentText(stripCitationText(text)) }
    Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (isStreaming && text.isEmpty()) {
            Row(
                modifier = Modifier.padding(top = 2.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                AssistantGutter()
                AuroraThinking()
            }
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
                AssistantGutter()
                Column(
                    modifier = Modifier.widthIn(max = 292.dp),
                    verticalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    InlineMarkdownText(displayText, showCaret = isStreaming && displayText.isNotBlank())
                    if (citations.isNotEmpty()) {
                        FlowRow(
                            horizontalArrangement = Arrangement.spacedBy(7.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            citations.forEachIndexed { index, citation ->
                                SourceCitationPill(
                                    index = index + 1,
                                    citation = citation,
                                    onOpenDocument = onOpenDocument,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun UserInitials() {
    val colors = AxonTheme.colors
    Box(
        modifier = Modifier
            .size(28.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(Brush.linearGradient(listOf(colors.accentDeep, colors.accentPrimary))),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            "JM",
            color = Color(0xFF06131C),
            fontSize = 10.6.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = AxonTheme.fonts.body,
        )
    }
}

@Composable
private fun AssistantGutter() {
    val colors = AxonTheme.colors
    Box(
        modifier = Modifier
            .size(26.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(colors.tint(colors.accentPrimary, 10, colors.control))
            .border(1.dp, colors.tint(colors.accentPrimary, 22, colors.control), RoundedCornerShape(8.dp)),
        contentAlignment = Alignment.Center,
    ) {
        AxonMarkGlyph(Modifier.size(16.dp))
    }
}

@Composable
private fun InlineMarkdownText(text: String, showCaret: Boolean = false) {
    val colors = AxonTheme.colors
    val code = colors.toneOf(AxonTone.Rose)
    val parts = remember(text) { text.split('`') }
    Text(
        text = buildAnnotatedString {
            parts.forEachIndexed { index, part ->
                if (index % 2 == 0) {
                    append(part)
                } else {
                    withStyle(
                        SpanStyle(
                            color = code.fg,
                            background = colors.tint(code.base, 9, colors.control),
                            fontFamily = AxonTheme.fonts.mono,
                            fontSize = 12.4.sp,
                        ),
                    ) {
                        append(part)
                    }
                }
            }
            if (showCaret) {
                withStyle(SpanStyle(color = colors.accentStrong)) {
                    append("▍")
                }
            }
        },
        color = colors.textPrimary.copy(alpha = 0.92f),
        fontSize = 13.5.sp,
        lineHeight = 20.2.sp,
        fontFamily = AxonTheme.fonts.body,
    )
}

@Composable
private fun rememberCitations(text: String): List<ChatCitation> = remember(text) {
    extractedCitations(text)
}

private data class ChatCitation(val label: String, val url: String?)

private val answerMetadataBlockRegex = Regex(
    "(?is)\\n*#{1,3}\\s*(?:Citation\\s+Validation\\s+Failed|Retrieved\\s+Sources|Sources)\\s*\\n.*$",
)

internal fun stripCitationText(text: String): String =
    text
        .replace(answerMetadataBlockRegex, "")
        .replace(Regex("\\s*\\[S\\d+\\]"), "")
        .trim()

internal fun extractedCitationLabels(text: String): List<String> {
    return extractedCitations(text).map { it.label }
}

private fun extractedCitations(text: String): List<ChatCitation> {
    val markers = Regex("\\[S\\d+\\]")
        .findAll(text)
        .map { ChatCitation(label = it.value.removeSurrounding("[", "]"), url = null) }
        .toList()

    val urls = answerMetadataBlockRegex.find(text)
        ?.value
        ?.lineSequence()
        ?.map { it.trim().removePrefix("-").trim() }
        ?.filter { it.startsWith("http://") || it.startsWith("https://") }
        ?.map { ChatCitation(label = compactSourceLabel(it), url = it) }
        ?.toList()
        ?: emptyList()

    return (urls + markers).distinctBy { it.url ?: it.label }.take(6)
}

private fun compactSourceLabel(url: String): String =
    runCatching {
        val withoutScheme = url.substringAfter("://")
        val host = withoutScheme.substringBefore("/")
        host.removePrefix("www.").substringBeforeLast(".").takeIf { it.isNotBlank() } ?: host
    }.getOrElse { "source" }

@Composable
private fun SourceCitationPill(
    index: Int,
    citation: ChatCitation,
    onOpenDocument: (String) -> Unit,
) {
    val colors = AxonTheme.colors
    val chip = colors.toneOf(AxonTone.Cyan)
    val clickModifier = citation.url?.let { url -> Modifier.clickable { onOpenDocument(url) } } ?: Modifier
    Row(
        modifier = Modifier
            .then(clickModifier)
            .background(colors.tint(chip.base, 7, colors.control), RoundedCornerShape(999.dp))
            .border(1.dp, colors.tint(chip.base, 15, colors.control), RoundedCornerShape(999.dp))
            .padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("$index", color = chip.fg.copy(alpha = 0.84f), fontSize = 9.6.sp, fontWeight = FontWeight.Bold, fontFamily = AxonTheme.fonts.mono)
        Text(
            citation.label,
            color = colors.textMuted.copy(alpha = 0.78f),
            fontSize = 9.6.sp,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
