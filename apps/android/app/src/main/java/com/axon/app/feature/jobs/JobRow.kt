package com.axon.app.feature.jobs

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.axon.app.data.repository.JobUi
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone

/**
 * Map a server-side job status string onto the closest [AuroraStatusTone].
 *
 * `internal` so [JobsToneTest] can import and verify the mapping directly,
 * rather than maintaining a tautological replica in the test file.
 */
internal fun toneFor(status: String): AuroraStatusTone = when (status.lowercase()) {
    "pending", "queued"      -> AuroraStatusTone.Queued
    "running", "in_progress" -> AuroraStatusTone.Syncing
    "completed", "succeeded" -> AuroraStatusTone.Online
    "failed", "error"        -> AuroraStatusTone.Error
    "cancelled", "canceled"  -> AuroraStatusTone.Offline
    else                     -> AuroraStatusTone.Degraded
}

internal val CANCELABLE_STATUSES = setOf("pending", "queued", "running", "in_progress")

/** Single job row: target/url/id label + status indicator + optional Cancel button. */
@Composable
fun JobRow(job: JobUi, onCancel: (() -> Unit)? = null) {
    val cancelable = job.status.lowercase() in CANCELABLE_STATUSES
    AuroraCard(modifier = Modifier.fillMaxWidth(), variant = AuroraCardVariant.Outlined) {
        Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Text(
                    text = job.target ?: job.url ?: job.id,
                    style = MaterialTheme.typography.labelMedium,
                    modifier = Modifier.weight(1f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                AuroraStatusIndicator(tone = toneFor(job.status), label = job.status)
            }
            job.errorText?.let {
                Text(
                    text = "error: $it",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            if (cancelable && onCancel != null) {
                AuroraButton(
                    onClick = onCancel,
                    variant = AuroraButtonVariant.Outlined,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("Cancel")
                }
            }
        }
    }
}
