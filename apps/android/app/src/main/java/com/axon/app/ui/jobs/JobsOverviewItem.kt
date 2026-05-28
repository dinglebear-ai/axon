package com.axon.app.ui.jobs

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
                color = Color(0xFFE6F4FB),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                job.status,
                style = MaterialTheme.typography.labelSmall,
                color = Color(0xFF4A6374),
            )
        }
    }
}

private fun statusColor(status: String): Color = when (status) {
    "running", "processing" -> Color(0xFF29B6F6)   // cyan — active
    "pending"               -> Color(0xFFC6A36B)   // amber — waiting
    "done"                  -> Color(0xFF4CAF50)   // green — done
    "failed", "error"       -> Color(0xFFEF5350)   // red — failed
    else                    -> Color(0xFF4A6374)   // muted — unknown
}

private fun jobTarget(job: JobUi): String =
    job.url ?: job.target ?: job.id.take(12)
