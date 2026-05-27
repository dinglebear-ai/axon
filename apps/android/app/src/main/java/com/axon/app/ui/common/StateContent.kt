package com.axon.app.ui.common

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.Alignment
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

/**
 * Error callout shared across screens.
 * Pass [onRetry] to show a "Retry" button below the callout (e.g. after configuring credentials).
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
        )
        if (onRetry != null) {
            Spacer(Modifier.height(8.dp))
            OutlinedButton(onClick = onRetry, modifier = Modifier.fillMaxWidth()) {
                Text("Retry")
            }
        }
    }
}
