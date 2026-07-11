package com.axon.app.feature.ask

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.nav.AxonMarkGlyph
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf
import tv.tootie.aurora.components.AuroraAvatar
import tv.tootie.aurora.components.AuroraAvatarSize
import tv.tootie.aurora.components.AuroraInlineCitation
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

internal data class MessageAction(val icon: ImageVector, val desc: String, val onClick: () -> Unit)

/** Reveal-on-tap footer: timestamp + small action buttons under a message. */
@Composable
internal fun MessageActions(
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
internal fun UserInitials() {
    // Aurora avatar (cyan, circular initials) — aligned with the design-system
    // avatar gallery. A two-word name yields the "JM" monogram.
    AuroraAvatar(name = "Jacob Magar", size = AuroraAvatarSize.Sm)
}

@Composable
internal fun AssistantGutter() {
    val colors = AxonTheme.colors
    Box(
        modifier = Modifier
            .size(26.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(colors.tint(colors.accentPink, 10, colors.control))
            .border(1.dp, colors.tint(colors.accentPink, 22, colors.control), RoundedCornerShape(8.dp)),
        contentAlignment = Alignment.Center,
    ) {
        AxonMarkGlyph(Modifier.size(16.dp))
    }
}

/** Wordless "thinking" indicator: three accent dots pulsing in sequence. */
@Composable
internal fun ThinkingDots() {
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
                    .background(colors.accentPinkStrong),
            )
        }
    }
}

@Composable
internal fun InlineMarkdownText(
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
                            fontSize = 13.sp,
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
        fontSize = 15.4.sp,
        lineHeight = 22.6.sp,
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
