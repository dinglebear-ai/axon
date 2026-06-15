package com.axon.app.ui.ask

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Autorenew
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.DataObject
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material.icons.rounded.ExpandMore
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Storage
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
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.AuroraStatusDot
import com.axon.app.ui.common.DotState
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.nav.AxonMarkGlyph
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf
import kotlinx.collections.immutable.toImmutableList
import tv.tootie.aurora.components.AuroraAvatar
import tv.tootie.aurora.components.AuroraAvatarSize
import tv.tootie.aurora.components.AuroraInlineCitation
import tv.tootie.aurora.components.AuroraSource
import tv.tootie.aurora.components.AuroraSources
import java.net.URI
import java.net.URLDecoder
import java.nio.charset.StandardCharsets
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Agent-style tool-call pill (mirrors the Aurora Tool Calls component): a
 * leading status dot, tool icon, and mono tool name. Collapsed to a compact
 * pill by default; tap to expand the input/output.
 */
@Composable
fun ActivityRailRow(item: ChatItem.Activity, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    var expanded by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(if (expanded) 11.dp else 9.dp)
    val chevronRotation by animateFloatAsState(
        targetValue = if (expanded) 180f else 0f,
        animationSpec = tween(durationMillis = 200),
        label = "tool-chevron",
    )
    Column(
        modifier = modifier
            .padding(start = 34.dp, top = 4.dp, bottom = 4.dp)
            .widthIn(max = 322.dp)
            .clip(shape)
            .background(colors.tint(colors.accentPrimary, 5, colors.control), shape)
            .border(1.dp, colors.tint(colors.accentPrimary, 14, colors.control), shape)
            .clickable(remember { MutableInteractionSource() }, indication = null) { expanded = !expanded }
            .animateContentSize()
            .padding(horizontal = 11.dp, vertical = 7.dp),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            AuroraStatusDot(if (item.done) DotState.Done else DotState.Running, size = 7.dp)
            Icon(
                activityIcon(item.name),
                contentDescription = null,
                tint = colors.textMuted.copy(alpha = 0.82f),
                modifier = Modifier.size(13.dp),
            )
            Text(
                item.name,
                color = colors.textPrimary,
                fontSize = 12.5.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
            )
            Icon(
                Icons.Rounded.ExpandMore,
                contentDescription = if (expanded) "Collapse" else "Expand",
                tint = colors.textMuted.copy(alpha = 0.7f),
                modifier = Modifier.size(15.dp).graphicsLayer { rotationZ = chevronRotation },
            )
        }
        if (expanded) {
            ToolKv("input", item.arg, colors.textMuted)
            ToolKv("output", item.result, if (item.done) colors.success else colors.accentStrong)
        }
    }
}

@Composable
private fun ToolKv(label: String, value: String, valueColor: Color) {
    val colors = AxonTheme.colors
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            label,
            color = colors.textMuted.copy(alpha = 0.58f),
            fontSize = 10.5.sp,
            lineHeight = 14.sp,
            fontFamily = AxonTheme.fonts.mono,
            modifier = Modifier.width(48.dp),
        )
        Text(
            value.ifBlank { "—" },
            color = valueColor.copy(alpha = 0.9f),
            fontSize = 11.sp,
            lineHeight = 14.sp,
            fontFamily = AxonTheme.fonts.mono,
            modifier = Modifier.weight(1f, fill = false),
        )
    }
}

private fun activityIcon(name: String): ImageVector {
    val n = name.lowercase()
    return when {
        n.contains("retriev") -> Icons.Rounded.Storage
        n.contains("search") -> Icons.Rounded.Search
        n.contains("ask") || n.contains("synth") || n.contains("answer") || n.contains("generat") -> Icons.Rounded.AutoAwesome
        else -> Icons.Rounded.DataObject
    }
}

@Composable
fun UserBubble(
    text: String,
    modifier: Modifier = Modifier,
    timestamp: Long? = null,
    onEdit: (() -> Unit)? = null,
    onCopy: (() -> Unit)? = null,
) {
    val colors = AxonTheme.colors
    val displayText = remember(text) { displayUserText(text) }
    val shape = RoundedCornerShape(topStart = 17.dp, topEnd = 6.dp, bottomStart = 17.dp, bottomEnd = 17.dp)
    var revealed by remember { mutableStateOf(false) }
    // Vertical gradient + top-edge highlight border + a soft cyan glow give the
    // bubble depth instead of a flat fill.
    val fillBrush = remember(colors) {
        Brush.verticalGradient(
            listOf(
                colors.tint(colors.accentPrimary, 28, colors.control),
                colors.tint(colors.accentPrimary, 13, colors.control),
            ),
        )
    }
    val borderBrush = remember(colors) {
        Brush.verticalGradient(
            listOf(
                colors.tint(colors.accentPrimary, 50, colors.control),
                colors.tint(colors.accentPrimary, 22, colors.control),
            ),
        )
    }
    Column(modifier = modifier.fillMaxWidth(), horizontalAlignment = Alignment.End) {
        Row(
            modifier = Modifier.widthIn(max = 300.dp),
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
                    fontSize = 15.sp,
                    lineHeight = 20.5.sp,
                    fontFamily = AxonTheme.fonts.body,
                    modifier = Modifier
                        .shadow(
                            elevation = 5.dp,
                            shape = shape,
                            ambientColor = colors.accentPrimary.copy(alpha = 0.4f),
                            spotColor = colors.accentPrimary.copy(alpha = 0.5f),
                        )
                        .clip(shape)
                        .background(fillBrush, shape)
                        .border(1.dp, borderBrush, shape)
                        .clickable(remember { MutableInteractionSource() }, indication = null) { revealed = !revealed }
                        .padding(horizontal = 14.dp, vertical = 10.dp),
                )
            }
            UserInitials()
        }
        if (revealed) {
            MessageActions(
                timestamp = timestamp,
                actions = listOfNotNull(
                    onEdit?.let { MessageAction(Icons.Rounded.Edit, "Edit message", it) },
                    onCopy?.let { MessageAction(Icons.Rounded.ContentCopy, "Copy message", it) },
                ),
                modifier = Modifier.padding(end = 35.dp),
            )
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
    timestamp: Long? = null,
    onCopy: (() -> Unit)? = null,
    onRegenerate: (() -> Unit)? = null,
) {
    val colors = AxonTheme.colors
    val answerSources = remember(text) { parseAnswerSources(text) }
    val sources = remember(answerSources) {
        answerSources.map { AuroraSource(title = it.title, url = it.url) }.toImmutableList()
    }
    // Map each backend source number (`[S15]`) to its display index + URL so inline
    // citations renumber to match the carousel (1..k) and link to the right doc,
    // even when the backend's source numbering isn't a contiguous 1..k.
    val citationLinks = remember(answerSources) {
        answerSources.mapIndexedNotNull { i, s -> s.num?.let { n -> n to CitationLink(i + 1, s.url) } }.toMap()
    }
    // Keep inline `[Sn]` markers in the prose when we have sources to back them
    // (rendered as Aurora inline-citation badges); otherwise strip the dangling
    // markers so no bare `[S1]` leaks into the text.
    val displayText = remember(text, sources.isEmpty()) {
        val base = if (sources.isEmpty()) stripCitationText(text) else stripSourcesBlock(text)
        humanizeJsonFragmentText(base)
    }
    val shape = RoundedCornerShape(topStart = 6.dp, topEnd = 17.dp, bottomStart = 17.dp, bottomEnd = 17.dp)
    var revealed by remember { mutableStateOf(false) }
    // Subtle vertical gradient + a brighter top hairline read as a lit glass
    // panel rather than a flat block.
    val fillBrush = remember(colors) {
        Brush.verticalGradient(
            listOf(
                colors.panelStrong.copy(alpha = 0.72f),
                colors.panelMedium.copy(alpha = 0.5f),
            ),
        )
    }
    val borderBrush = remember(colors) {
        Brush.verticalGradient(
            listOf(
                colors.tint(colors.accentPrimary, 20, colors.panelStrong),
                colors.tint(colors.accentPrimary, 7, colors.panelStrong),
            ),
        )
    }
    Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (isStreaming && text.isEmpty()) {
            Row(
                modifier = Modifier.padding(top = 2.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                AssistantGutter()
                ThinkingDots()
            }
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
                AssistantGutter()
                Column(horizontalAlignment = Alignment.Start) {
                    Column(
                        modifier = Modifier
                            .widthIn(max = 300.dp)
                            .shadow(elevation = 4.dp, shape = shape)
                            .clip(shape)
                            .background(fillBrush, shape)
                            .border(1.dp, borderBrush, shape)
                            .clickable(remember { MutableInteractionSource() }, indication = null) { revealed = !revealed }
                            .padding(horizontal = 14.dp, vertical = 12.dp),
                        verticalArrangement = Arrangement.spacedBy(9.dp),
                    ) {
                        InlineMarkdownText(
                            text = displayText,
                            citationLinks = citationLinks,
                            onOpenDocument = onOpenDocument,
                            showCaret = isStreaming && displayText.isNotBlank(),
                        )
                        if (sources.isNotEmpty()) {
                            // Sources land softly once the answer settles rather
                            // than snapping in at the Done event.
                            var sourcesShown by remember(sources) { mutableStateOf(false) }
                            LaunchedEffect(sources) { sourcesShown = true }
                            AnimatedVisibility(
                                visible = sourcesShown,
                                enter = fadeIn(tween(280)) + expandVertically(tween(260)),
                            ) {
                                AuroraSources(
                                    sources = sources,
                                    onSourceClick = { onOpenDocument(it.url) },
                                    modifier = Modifier.fillMaxWidth(),
                                )
                            }
                        }
                    }
                    if (revealed && !isStreaming) {
                        MessageActions(
                            timestamp = timestamp,
                            actions = listOfNotNull(
                                onCopy?.let { MessageAction(Icons.Rounded.ContentCopy, "Copy message", it) },
                                onRegenerate?.let { MessageAction(Icons.Rounded.Autorenew, "Regenerate response", it) },
                            ),
                            modifier = Modifier.padding(start = 2.dp),
                        )
                    }
                }
            }
        }
    }
}

private data class MessageAction(val icon: ImageVector, val desc: String, val onClick: () -> Unit)

/** Reveal-on-tap footer: timestamp + small action buttons under a message. */
@Composable
private fun MessageActions(
    timestamp: Long?,
    actions: List<MessageAction>,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    Row(
        modifier = modifier.padding(top = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        if (timestamp != null) {
            Text(
                text = remember(timestamp) { formatMessageTime(timestamp) },
                color = colors.textMuted.copy(alpha = 0.6f),
                fontSize = 10.5.sp,
                fontFamily = AxonTheme.fonts.mono,
                modifier = Modifier.padding(end = 5.dp),
            )
        }
        actions.forEach { action ->
            Box(
                modifier = Modifier
                    .size(30.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .pressScale(onClick = action.onClick),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    action.icon,
                    contentDescription = action.desc,
                    tint = colors.textMuted.copy(alpha = 0.85f),
                    modifier = Modifier.size(16.dp),
                )
            }
        }
    }
}

private fun formatMessageTime(ts: Long): String =
    SimpleDateFormat("h:mm a", Locale.getDefault()).format(Date(ts))

@Composable
private fun UserInitials() {
    // Aurora avatar (cyan, circular initials) — aligned with the design-system
    // avatar gallery. A two-word name yields the "JM" monogram.
    AuroraAvatar(name = "Jacob Magar", size = AuroraAvatarSize.Sm)
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

/** Wordless "thinking" indicator: three accent dots pulsing in sequence. */
@Composable
private fun ThinkingDots() {
    val colors = AxonTheme.colors
    val transition = rememberInfiniteTransition(label = "thinking")
    Row(
        modifier = Modifier.padding(top = 5.dp, bottom = 5.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        repeat(3) { index ->
            val pulse by transition.animateFloat(
                initialValue = 0.28f,
                targetValue = 1f,
                animationSpec = infiniteRepeatable(
                    animation = tween(durationMillis = 560, delayMillis = index * 150, easing = LinearEasing),
                    repeatMode = RepeatMode.Reverse,
                ),
                label = "dot-$index",
            )
            Box(
                modifier = Modifier
                    .size(8.dp)
                    .graphicsLayer {
                        alpha = pulse
                        val s = 0.7f + 0.3f * pulse
                        scaleX = s
                        scaleY = s
                    }
                    .clip(CircleShape)
                    .background(colors.accentStrong),
            )
        }
    }
}

@Composable
private fun InlineMarkdownText(
    text: String,
    citationLinks: Map<Int, CitationLink> = emptyMap(),
    onOpenDocument: (String) -> Unit = {},
    showCaret: Boolean = false,
) {
    val colors = AxonTheme.colors
    val code = colors.toneOf(AxonTone.Rose)
    val parts = remember(text) { text.split('`') }
    // A blinking caret reads as a live typing indicator while the answer streams.
    val caretAlpha = if (showCaret) {
        val blink = rememberInfiniteTransition(label = "caret")
        val a by blink.animateFloat(
            initialValue = 1f,
            targetValue = 0.15f,
            animationSpec = infiniteRepeatable(
                animation = tween(640, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "caret-blink",
        )
        a
    } else {
        1f
    }

    // Each backend `[Sn]` that maps to a known source becomes an inline Aurora
    // citation badge (renumbered to its display index); unmatched markers fall
    // through as plain text.
    val inlineContent = remember(citationLinks) {
        citationLinks.entries.associate { (num, link) ->
            "cite_$num" to InlineTextContent(
                placeholder = Placeholder(
                    width = if (link.displayIndex >= 10) 2.9.em else 2.2.em,
                    height = 1.5.em,
                    placeholderVerticalAlign = PlaceholderVerticalAlign.Center,
                ),
            ) {
                AuroraInlineCitation(number = link.displayIndex, onClick = { onOpenDocument(link.url) })
            }
        }
    }

    Text(
        text = buildAnnotatedString {
            parts.forEachIndexed { index, part ->
                if (index % 2 == 0) {
                    appendProseWithCitations(part, citationLinks.keys)
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
                withStyle(SpanStyle(color = colors.accentStrong.copy(alpha = caretAlpha))) {
                    append("▍")
                }
            }
        },
        inlineContent = inlineContent,
        color = colors.textPrimary.copy(alpha = 0.92f),
        fontSize = 14.5.sp,
        lineHeight = 21.5.sp,
        fontFamily = AxonTheme.fonts.body,
    )
}

/** Append a prose run, swapping each `[Sn]` marker with a known source for an inline-citation placeholder. */
private fun AnnotatedString.Builder.appendProseWithCitations(part: String, validNums: Set<Int>) {
    var cursor = 0
    inlineCitationMarkerRegex.findAll(part).forEach { match ->
        append(part.substring(cursor, match.range.first))
        val n = match.groupValues[1].toIntOrNull()
        if (n != null && n in validNums) {
            appendInlineContent("cite_$n", match.value)
        } else {
            append(match.value)
        }
        cursor = match.range.last + 1
    }
    append(part.substring(cursor))
}

/** A resolvable inline citation: the 1-based display number shown in the badge + the doc URL it opens. */
private data class CitationLink(val displayIndex: Int, val url: String)

/** One parsed line from the answer's Sources block: backend `[Sn]` number (if any), URL, and a short title. */
private data class AnswerSource(val num: Int?, val url: String, val title: String)

private val sourceNumRegex = Regex("\\[S(\\d+)\\]")

/** Parse the trailing Sources block into ordered, de-duplicated sources with their backend numbers. */
private fun parseAnswerSources(text: String): List<AnswerSource> {
    val block = answerMetadataBlockRegex.find(text)?.value ?: return emptyList()
    return block.lineSequence()
        .mapNotNull { line ->
            val url = sourceUrlRegex.find(line)?.value?.trimEnd('.', ',', ')', '"', '\'') ?: return@mapNotNull null
            val num = sourceNumRegex.find(line)?.groupValues?.get(1)?.toIntOrNull()
            AnswerSource(num = num, url = url, title = sourceDisplayTitle(url))
        }
        .distinctBy { it.url }
        .take(12)
        .toList()
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

/** Strip only the trailing sources/metadata block, leaving inline `[Sn]` markers intact. */
internal fun stripSourcesBlock(text: String): String =
    text.replace(answerMetadataBlockRegex, "").trim()

private val inlineCitationMarkerRegex = Regex("\\[S(\\d+)\\]")

internal fun extractedCitationLabels(text: String): List<String> {
    return extractedCitations(text).map { it.label }
}

private val sourceUrlRegex = Regex("https?://[^\\s)\\]\"']+")

private fun extractedCitations(text: String): List<ChatCitation> {
    val markers = Regex("\\[S\\d+\\]")
        .findAll(text)
        .map { ChatCitation(label = it.value.removeSurrounding("[", "]"), url = null) }
        .toList()

    // The backend emits sources as `- [S1] https://…` (and older runs as
    // `- https://…`); pull the URL out of each line regardless of the `[Sn]`
    // prefix so they stay clickable.
    val urls = answerMetadataBlockRegex.find(text)
        ?.value
        ?.lineSequence()
        ?.mapNotNull { line -> sourceUrlRegex.find(line)?.value?.trimEnd('.', ',', ')', '"', '\'') }
        ?.map { ChatCitation(label = compactSourceLabel(it), url = it) }
        ?.toList()
        ?: emptyList()

    return (urls + markers).distinctBy { it.url ?: it.label }.take(8)
}

private fun compactSourceLabel(url: String): String =
    runCatching {
        val withoutScheme = url.substringAfter("://")
        val host = withoutScheme.substringBefore("/")
        host.removePrefix("www.").substringBeforeLast(".").takeIf { it.isNotBlank() } ?: host
    }.getOrElse { "source" }

/** Carousel-facing title: the last path segment (e.g. `scrape.md`), else the host stem. */
private fun sourceDisplayTitle(url: String): String =
    runCatching {
        URI(url).path.orEmpty().trim('/').split('/').lastOrNull { it.isNotBlank() }
    }.getOrNull()?.takeIf { it.isNotBlank() } ?: compactSourceLabel(url)
