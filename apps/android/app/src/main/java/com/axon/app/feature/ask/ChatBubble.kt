package com.axon.app.feature.ask

import android.view.HapticFeedbackConstants
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Autorenew
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material.icons.rounded.ExpandMore
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.AuroraStatusDot
import com.axon.app.ui.common.DotState
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import kotlinx.collections.immutable.toImmutableList
import tv.tootie.aurora.components.AuroraSource
import tv.tootie.aurora.components.AuroraSources
import java.net.URLDecoder
import java.nio.charset.StandardCharsets

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

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun UserBubble(
    text: String,
    modifier: Modifier = Modifier,
    timestamp: Long? = null,
    showAvatar: Boolean = true,
    onEdit: (() -> Unit)? = null,
    onCopy: (() -> Unit)? = null,
) {
    val colors = AxonTheme.colors
    val view = LocalView.current
    val displayText = remember(text) { displayUserText(text) }
    val shape = RoundedCornerShape(17.dp)
    var revealed by remember { mutableStateOf(false) }
    val fillBrush = remember(colors) {
        Brush.verticalGradient(
            listOf(
                colors.tint(colors.accentPrimary, 28, colors.control),
                colors.tint(colors.accentPrimary, 13, colors.control),
            ),
        )
    }
    val borderTop by animateColorAsState(
        colors.tint(colors.accentPrimary, if (revealed) 66 else 50, colors.control),
        animationSpec = tween(200), label = "ub-border-top",
    )
    val borderBottom by animateColorAsState(
        colors.tint(colors.accentPrimary, if (revealed) 34 else 22, colors.control),
        animationSpec = tween(200), label = "ub-border-bottom",
    )
    val elevation by animateDpAsState(if (revealed) 9.dp else 5.dp, label = "ub-elev")
    val glowAlpha by animateFloatAsState(if (revealed) 0.62f else 0.45f, label = "ub-glow")
    Column(modifier = modifier.fillMaxWidth(), horizontalAlignment = Alignment.End) {
        Row(
            modifier = Modifier.widthIn(max = 328.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.Top,
        ) {
            Column(
                modifier = Modifier.weight(1f, fill = false),
                horizontalAlignment = Alignment.End,
            ) {
                Text(
                    text = displayText,
                    color = colors.textPrimary,
                    fontSize = 15.6.sp,
                    lineHeight = 21.8.sp,
                    fontFamily = AxonTheme.fonts.body,
                    modifier = Modifier
                        .shadow(
                            elevation = elevation,
                            shape = shape,
                            ambientColor = colors.accentPrimary.copy(alpha = glowAlpha - 0.1f),
                            spotColor = colors.accentPrimary.copy(alpha = glowAlpha),
                        )
                        .clip(shape)
                        .background(fillBrush, shape)
                        .border(1.dp, Brush.verticalGradient(listOf(borderTop, borderBottom)), shape)
                        .combinedClickable(
                            interactionSource = remember { MutableInteractionSource() },
                            indication = null,
                            onClick = { revealed = !revealed },
                            onLongClick = {
                                view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                                revealed = true
                            },
                        )
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                )
            }
            if (showAvatar) UserInitials() else Spacer(Modifier.width(24.dp))
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

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun AxonBubble(
    text: String,
    isStreaming: Boolean = false,
    modifier: Modifier = Modifier,
    onOpenDocument: (String) -> Unit = {},
    timestamp: Long? = null,
    showAvatar: Boolean = true,
    onCopy: (() -> Unit)? = null,
    onRegenerate: (() -> Unit)? = null,
) {
    val colors = AxonTheme.colors
    val view = LocalView.current
    val answerSources = remember(text) { parseAnswerSources(text) }
    val sources = remember(answerSources) {
        answerSources.map { AuroraSource(title = it.title, url = it.url) }.toImmutableList()
    }
    // Map each backend source number (`[S15]`) to its display index + URL so inline
    // citations renumber to match the carousel (1..k) and link to the right doc,
    // even when the backend's source numbering isn't a contiguous 1..k.
    val citationLinks = remember(answerSources) { buildCitationLinks(answerSources) }
    // Keep inline `[Sn]` markers in the prose when we have sources to back them
    // (rendered as Aurora inline-citation badges); otherwise strip the dangling
    // markers so no bare `[S1]` leaks into the text.
    val displayText = remember(text, sources.isEmpty()) {
        val base = if (sources.isEmpty()) stripCitationText(text) else stripSourcesBlock(text)
        humanizeJsonFragmentText(base)
    }
    val shape = RoundedCornerShape(17.dp)
    var revealed by remember { mutableStateOf(false) }
    val isError = remember(text) { text.startsWith("Error:") }
    val accent = if (isError) colors.error else colors.accentPink
    val topStrength = when {
        revealed -> 34
        isStreaming -> 26
        else -> 20
    }
    val borderTop by animateColorAsState(
        colors.tint(accent, topStrength, colors.panelStrong),
        animationSpec = tween(240), label = "ab-border-top",
    )
    val borderBottom by animateColorAsState(
        colors.tint(accent, 7, colors.panelStrong),
        animationSpec = tween(240), label = "ab-border-bottom",
    )
    val elevation by animateDpAsState(if (revealed) 7.dp else 4.dp, label = "ab-elev")
    val fillBrush = remember(colors, accent) {
        Brush.verticalGradient(
            listOf(
                colors.tint(accent, 7, colors.panelStrong).copy(alpha = 0.74f),
                colors.tint(accent, 3, colors.panelMedium).copy(alpha = 0.52f),
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
                if (showAvatar) AssistantGutter() else Spacer(Modifier.width(26.dp))
                ThinkingDots()
            }
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(9.dp), verticalAlignment = Alignment.Top) {
                if (showAvatar) AssistantGutter() else Spacer(Modifier.width(26.dp))
                Column(horizontalAlignment = Alignment.Start) {
                    Column(
                        modifier = Modifier
                            .widthIn(max = 328.dp)
                            .shadow(elevation = elevation, shape = shape)
                            .clip(shape)
                            .background(fillBrush, shape)
                            .border(1.dp, Brush.verticalGradient(listOf(borderTop, borderBottom)), shape)
                            .combinedClickable(
                                interactionSource = remember { MutableInteractionSource() },
                                indication = null,
                                onClick = { revealed = !revealed },
                                onLongClick = {
                                    view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                                    revealed = true
                                },
                            )
                            .padding(horizontal = 16.dp, vertical = 14.dp),
                        verticalArrangement = Arrangement.spacedBy(11.dp),
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
