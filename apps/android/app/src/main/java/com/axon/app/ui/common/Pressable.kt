package com.axon.app.ui.common

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.scale
import androidx.compose.ui.semantics.Role

/**
 * Indication-free clickable that adds a subtle spring press-scale, matching the
 * app's custom rippleless interaction style. The bare
 * `.clickable(remember { MutableInteractionSource() }, indication = null, …)`
 * idiom is repeated across the shell and every custom row, but it gives a tap
 * zero tactile feedback. [pressScale] keeps the look (no ripple) while restoring
 * a physical press response that springs back on release.
 *
 * Scale is the lightest-touch feedback available — it never shifts layout
 * (it transforms at draw time) so it is safe on rows, tiles, and icon buttons.
 */
fun Modifier.pressScale(
    enabled: Boolean = true,
    pressedScale: Float = 0.96f,
    role: Role? = null,
    onClick: () -> Unit,
): Modifier = composed {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (pressed && enabled) pressedScale else 1f,
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessHigh,
        ),
        label = "press-scale",
    )
    scale(scale).clickable(
        interactionSource = interaction,
        indication = null,
        enabled = enabled,
        role = role,
        onClick = onClick,
    )
}
