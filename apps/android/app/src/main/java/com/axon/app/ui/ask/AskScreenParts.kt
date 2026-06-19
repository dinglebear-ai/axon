package com.axon.app.ui.ask

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.nav.AxonMarkGlyph
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

@Composable
internal fun AskModeSwitch(
    mode: ConversationMode,
    onModeChange: (ConversationMode) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(999.dp)
    val entries = ConversationMode.entries
    val selectedIndex = entries.indexOf(mode).coerceAtLeast(0)
    val innerSpacing = 3.dp

    BoxWithConstraints(
        modifier = modifier
            .height(36.dp)
            .clip(shape)
            .background(colors.control.copy(alpha = 0.58f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.56f), shape)
            .padding(3.dp),
    ) {
        val segWidth = (maxWidth - innerSpacing * (entries.size - 1)) / entries.size
        // The selected pill glides between segments instead of teleporting.
        val indicatorX by animateDpAsState(
            targetValue = (segWidth + innerSpacing) * selectedIndex,
            animationSpec = spring(
                dampingRatio = Spring.DampingRatioMediumBouncy,
                stiffness = Spring.StiffnessMediumLow,
            ),
            label = "mode-indicator",
        )
        Box(
            modifier = Modifier
                .offset(x = indicatorX)
                .width(segWidth)
                .fillMaxHeight()
                .clip(shape)
                .background(colors.tint(colors.accentPrimary, 12, colors.control), shape)
                .border(1.dp, colors.tint(colors.accentPrimary, 28, colors.control), shape),
        )
        androidx.compose.foundation.layout.Row(
            horizontalArrangement = Arrangement.spacedBy(innerSpacing),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            entries.forEach { item ->
                val selected = item == mode
                val labelColor by animateColorAsState(
                    targetValue = if (selected) colors.accentStrong else colors.textMuted.copy(alpha = 0.78f),
                    animationSpec = tween(durationMillis = 200),
                    label = "mode-label",
                )
                Box(
                    modifier = Modifier
                        .width(segWidth)
                        .fillMaxHeight()
                        .pressScale(enabled = !selected) { onModeChange(item) },
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        item.label,
                        color = labelColor,
                        fontSize = 12.sp,
                        lineHeight = 16.sp,
                        fontWeight = FontWeight.SemiBold,
                        fontFamily = AxonTheme.fonts.body,
                    )
                }
            }
        }
    }
}

@Composable
internal fun EmptyAskState(
    modifier: Modifier = Modifier,
    suggestions: List<String> = emptyList(),
    onSuggestion: (String) -> Unit = {},
) {
    val colors = AxonTheme.colors
    // Soft breathing on the badge keeps the empty state feeling alive without
    // pulling focus — the node mark gently brightens and dims.
    val breathe = rememberInfiniteTransition(label = "empty-breathe")
    val glow by breathe.animateFloat(
        initialValue = 0.78f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(2200, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "empty-glow",
    )
    Box(
        modifier = modifier.padding(bottom = 72.dp),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier.widthIn(max = 340.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(13.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(58.dp)
                    .graphicsLayer { alpha = glow }
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
                fontSize = 17.sp,
                lineHeight = 23.sp,
                fontFamily = AxonTheme.fonts.display,
            )
            if (suggestions.isNotEmpty()) {
                Text(
                    "Start with",
                    color = colors.textMuted.copy(alpha = 0.7f),
                    fontSize = 13.sp,
                    lineHeight = 18.sp,
                    fontFamily = AxonTheme.fonts.body,
                )
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    suggestions.forEachIndexed { index, prompt ->
                        SuggestionChip(text = prompt, index = index, onClick = { onSuggestion(prompt) })
                    }
                }
            }
        }
    }
}

/** Tappable starter prompt that fades + rises in, staggered by [index]. */
@Composable
private fun SuggestionChip(text: String, index: Int, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }
    val anim by animateFloatAsState(
        targetValue = if (shown) 1f else 0f,
        animationSpec = tween(durationMillis = 360, delayMillis = 140 + index * 90, easing = LinearEasing),
        label = "chip-in",
    )
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(colors.control.copy(alpha = 0.025f), RoundedCornerShape(10.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.055f), RoundedCornerShape(10.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 10.dp)
            .graphicsLayer {
                alpha = anim
                translationY = (1f - anim) * 16f
            },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            text,
            color = colors.textMuted.copy(alpha = 0.94f),
            fontSize = 13.6.sp,
            lineHeight = 18.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            modifier = Modifier.weight(1f),
        )
        Icon(
            Icons.Rounded.ChevronRight,
            contentDescription = null,
            tint = colors.textMuted.copy(alpha = 0.56f),
            modifier = Modifier.size(15.dp),
        )
    }
}

/**
 * Floating "jump to latest" pill — surfaces only when the user has scrolled up
 * away from the bottom, so they can snap back to the live answer.
 */
@Composable
internal fun JumpToLatest(visible: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(999.dp)
    AnimatedVisibility(
        visible = visible,
        enter = fadeIn(tween(180)) + slideInVertically(tween(220)) { it / 2 },
        exit = fadeOut(tween(140)) + slideOutVertically(tween(180)) { it / 2 },
        modifier = modifier,
    ) {
        Row(
            modifier = Modifier
                .clip(shape)
                .background(colors.panelStrong.copy(alpha = 0.95f), shape)
                .border(1.dp, colors.tint(colors.accentPrimary, 22, colors.panelStrong), shape)
                .pressScale(onClick = onClick)
                .padding(start = 11.dp, end = 13.dp, top = 7.dp, bottom = 7.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            Icon(
                Icons.Rounded.KeyboardArrowDown,
                contentDescription = null,
                tint = colors.accentStrong,
                modifier = Modifier.size(16.dp),
            )
            Text(
                "Latest",
                color = colors.textPrimary.copy(alpha = 0.9f),
                fontSize = 12.sp,
                fontFamily = AxonTheme.fonts.body,
            )
        }
    }
}

/**
 * Overlay scroll thumb that reflects real scroll position and fades in only
 * while scrolling — replacing the previous static decorative thumb. Position is
 * approximated from the first visible item index (rows are near-uniform height),
 * and the thumb hides entirely when content fits on screen.
 */
@Composable
internal fun AuroraScrollThumb(listState: LazyListState, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    val trackHeight = 128.dp

    val metrics by remember(listState) {
        derivedStateOf {
            val info = listState.layoutInfo
            val total = info.totalItemsCount
            val visible = info.visibleItemsInfo.size
            val scrollable = total > 0 && visible in 1 until total
            val fraction = if (total > 1) {
                listState.firstVisibleItemIndex.toFloat() / (total - 1)
            } else {
                0f
            }
            val coverage = if (total > 0) (visible.toFloat() / total) else 1f
            Triple(scrollable, fraction.coerceIn(0f, 1f), coverage.coerceIn(0.18f, 1f))
        }
    }
    val (scrollable, fraction, coverage) = metrics

    val alpha by animateFloatAsState(
        targetValue = if (scrollable && listState.isScrollInProgress) 0.85f else 0f,
        animationSpec = tween(durationMillis = if (listState.isScrollInProgress) 120 else 420),
        label = "thumb-alpha",
    )

    val thumbHeight = trackHeight * coverage
    val offsetY = (trackHeight - thumbHeight) * fraction

    Box(
        modifier = modifier
            .width(4.dp)
            .height(trackHeight)
            .graphicsLayer { this.alpha = alpha },
        contentAlignment = Alignment.TopCenter,
    ) {
        Box(
            modifier = Modifier
                .offset(y = offsetY)
                .width(4.dp)
                .height(thumbHeight)
                .clip(RoundedCornerShape(999.dp))
                .background(colors.borderStrong.copy(alpha = 0.85f)),
        )
    }
}
