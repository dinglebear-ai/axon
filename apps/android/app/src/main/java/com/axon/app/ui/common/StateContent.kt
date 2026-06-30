package com.axon.app.ui.common

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ErrorOutline
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonTheme
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraEmptyState
import tv.tootie.aurora.components.AuroraThinking

/**
 * Centered loading indicator shared across screens. Callers in a Column/Row
 * supply `Modifier.weight(1f)` to fill the remaining space when desired.
 */
@Composable
fun LoadingContent(label: String, modifier: Modifier = Modifier) {
    Box(
        modifier = Modifier.fillMaxWidth().then(modifier),
        contentAlignment = Alignment.Center,
    ) {
        AuroraThinking(label = label)
    }
}

/**
 * Error callout shared across screens.
 * Pass [onRetry] to show a "Retry" button below the callout.
 */
@Composable
fun ErrorContent(
    message: String,
    modifier: Modifier = Modifier,
    onRetry: (() -> Unit)? = null,
) {
    val readableMessage = humanizeJsonFragmentText(message)
    Column(modifier = Modifier.fillMaxWidth().then(modifier)) {
        AuroraCallout(
            title = "Error",
            message = readableMessage,
            variant = AuroraCalloutVariant.Error,
            modifier = Modifier.fillMaxWidth(),
            icon = {
                Icon(
                    imageVector = Icons.Filled.ErrorOutline,
                    contentDescription = null,
                )
            },
        )
        if (onRetry != null) {
            Spacer(Modifier.height(8.dp))
            AuroraButton(
                onClick = onRetry,
                modifier = Modifier.fillMaxWidth(),
            ) {
                androidx.compose.material3.Text("Retry")
            }
        }
    }
}

/**
 * Renders a [Resource]-driven state holder via a single `when` block. Eliminates
 * the per-screen Idle/Loading/Error/Ready boilerplate plus the unchecked-cast
 * dance that arises from matching `is Resource.Ready<*>`. Callers supply the
 * Ready branch with a typed [value] — smart-cast is preserved.
 *
 * @param idle Defaults to the Loading branch so callers can omit it when the
 *   state never sits in [Resource.Idle] long enough to matter.
 * @param onRetry Wired to [ErrorContent]'s retry button when non-null.
 */
@Composable
fun <T> ResourceContent(
    state: Resource<T>,
    loadingLabel: String,
    onRetry: (() -> Unit)? = null,
    modifier: Modifier = Modifier,
    idle: @Composable () -> Unit = { LoadingContent(loadingLabel, modifier) },
    ready: @Composable (T) -> Unit,
) {
    // Crossfade on a stable *phase* token, not the state value: a Ready→Ready
    // value refresh keeps the same token and renders through without re-animating
    // (avoids flicker for callers that re-emit Ready). The branch content is read
    // from the live `state` inside the lambda so updated values still flow through.
    val phase = when (state) {
        Resource.Idle -> 0
        Resource.Loading -> 1
        is Resource.Error -> 2
        is Resource.Ready -> 3
    }
    Crossfade(targetState = phase, animationSpec = tween(durationMillis = 200), label = "ResourceContent") { token ->
        when (token) {
            0 -> idle()
            1 -> LoadingContent(loadingLabel, modifier)
            2 -> {
                val s = state
                if (s is Resource.Error) {
                    ErrorContent(message = s.message, modifier = modifier, onRetry = onRetry)
                }
            }
            else -> {
                val s = state
                if (s is Resource.Ready) {
                    ready(s.value)
                }
            }
        }
    }
}

/**
 * Empty state placeholder shared across screens.
 * Pass [icon] and [onAction] for a fully decorated empty state.
 */
@Composable
fun EmptyContent(
    title: String,
    modifier: Modifier = Modifier,
    description: String? = null,
    icon: ImageVector? = null,
    actionLabel: String? = null,
    onAction: (() -> Unit)? = null,
) {
    AuroraEmptyState(
        title = title,
        description = description,
        modifier = modifier,
        icon = icon?.let { { Icon(it, contentDescription = null) } },
        action = if (actionLabel != null && onAction != null) {
            { AuroraButton(onClick = onAction) { androidx.compose.material3.Text(actionLabel) } }
        } else null,
    )
}

/**
 * Compact recovery panel for empty/error/degraded states. Screens use this when
 * the user needs a concrete next action instead of a passive placeholder.
 */
@Composable
fun RecoveryActionCard(
    title: String,
    message: String,
    primaryLabel: String,
    onPrimary: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    secondaryLabel: String? = null,
    onSecondary: (() -> Unit)? = null,
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(10.dp)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .axonElevation(shape, AxonElevation.Card)
            .clip(shape)
            .background(colors.panelStrong.copy(alpha = 0.42f), shape)
            .border(1.dp, colors.borderStrong.copy(alpha = 0.22f), shape)
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            icon?.let {
                Icon(
                    imageVector = it,
                    contentDescription = null,
                    tint = colors.accentStrong,
                )
            }
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(
                    title,
                    color = colors.textPrimary,
                    fontSize = 15.sp,
                    lineHeight = 19.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    message,
                    color = colors.textMuted,
                    fontSize = 12.2.sp,
                    lineHeight = 17.sp,
                    fontFamily = AxonTheme.fonts.body,
                )
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(9.dp), modifier = Modifier.fillMaxWidth()) {
            AuroraButton(onClick = onPrimary, modifier = Modifier.weight(1f)) {
                Text(primaryLabel, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            if (secondaryLabel != null && onSecondary != null) {
                AuroraButton(onClick = onSecondary, modifier = Modifier.weight(1f)) {
                    Text(secondaryLabel, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}
