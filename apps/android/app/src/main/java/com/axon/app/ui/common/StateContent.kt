package com.axon.app.ui.common

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
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

/** Error callout shared across screens. */
@Composable
fun ErrorContent(message: String, modifier: Modifier = Modifier) {
    AuroraCallout(
        title = "Error",
        message = message,
        variant = AuroraCalloutVariant.Error,
        modifier = Modifier.fillMaxWidth().then(modifier),
    )
}
