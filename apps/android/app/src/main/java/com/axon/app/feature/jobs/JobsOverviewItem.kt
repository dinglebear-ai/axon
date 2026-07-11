package com.axon.app.feature.jobs

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Circle
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.data.repository.JobUi
import com.axon.app.ui.theme.AxonTheme

@Composable
fun JobsOverviewItem(job: JobUi, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 14.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(
            imageVector = Icons.Rounded.Circle,
            contentDescription = null,
            tint = statusColor(job.status),
            modifier = Modifier.size(8.dp),
        )
        Column(modifier = Modifier.weight(1f)) {
            Text(
                jobTarget(job),
                fontSize = 12.sp,
                color = AxonTheme.colors.textPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                job.status,
                style = MaterialTheme.typography.labelSmall,
                color = AxonTheme.colors.iconMuted,
            )
        }
    }
}

@Composable
private fun statusColor(status: String): Color = when (status) {
    "running", "processing" -> AxonTheme.colors.accentPrimary   // cyan — active
    "pending"               -> AxonTheme.colors.warn            // amber — waiting
    // Material green/red status tones (app-specific; intentionally NOT the muted
    // aurora success/error — these read as raw "done"/"failed" signals). No exact
    // lib token; single-use; kept literal to hold appearance exactly.
    "done"                  -> Color(0xFF4CAF50)   // green — done
    "failed", "error"       -> Color(0xFFEF5350)   // red — failed
    else                    -> AxonTheme.colors.iconMuted       // muted — unknown
}

private fun jobTarget(job: JobUi): String =
    job.url ?: job.target ?: job.id.take(12)
