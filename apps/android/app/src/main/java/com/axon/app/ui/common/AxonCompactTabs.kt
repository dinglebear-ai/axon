package com.axon.app.ui.common

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonTheme

@Composable
fun AxonCompactTabs(
    tabs: List<String>,
    selectedIndex: Int,
    onTabSelected: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(56.dp)
            .background(colors.navBg)
            .border(1.dp, colors.borderDefault),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        tabs.forEachIndexed { index, tab ->
            val selected = index == selectedIndex
            // Cross-fade the label and grow the underline in from its centre so
            // switching tabs reads as a slide of emphasis, not a hard repaint.
            val selectProgress by animateFloatAsState(
                targetValue = if (selected) 1f else 0f,
                animationSpec = tween(durationMillis = 220),
                label = "tab-select",
            )
            val labelColor by animateColorAsState(
                targetValue = if (selected) colors.accentPrimary else colors.textMuted,
                animationSpec = tween(durationMillis = 220),
                label = "tab-color",
            )
            Box(
                modifier = Modifier
                    .weight(1f)
                    .height(56.dp)
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = { onTabSelected(index) },
                    ),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    tab,
                    color = labelColor,
                    fontSize = 13.8.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 1,
                )
                Box(
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                        .height(3.dp)
                        .graphicsLayer {
                            alpha = selectProgress
                            scaleX = 0.32f + 0.68f * selectProgress
                        }
                        .background(colors.accentPrimary),
                )
            }
        }
    }
}
