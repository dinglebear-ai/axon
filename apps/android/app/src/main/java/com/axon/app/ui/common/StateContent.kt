package com.axon.app.ui.common

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ErrorOutline
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
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
    Column(modifier = Modifier.fillMaxWidth().then(modifier)) {
        AuroraCallout(
            title = "Error",
            message = message,
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
