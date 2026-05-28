package com.axon.app.ui.jobs

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Work
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.EmptyContent

@Composable
fun JobsDrawerContent(vm: JobsOverviewViewModel = viewModel()) {
    val jobs by vm.activeJobs.collectAsStateWithLifecycle()
    val error by vm.errorMessage.collectAsStateWithLifecycle()

    Column(modifier = Modifier.fillMaxWidth()) {
        // Header row with refresh button
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 14.dp, end = 4.dp, top = 8.dp, bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                "Active jobs",
                style = MaterialTheme.typography.labelMedium,
                color = Color(0xFFA7BCC9),
                modifier = Modifier.weight(1f),
            )
            IconButton(onClick = { vm.refresh() }, modifier = Modifier.size(32.dp)) {
                Icon(
                    Icons.Rounded.Refresh,
                    contentDescription = "Refresh",
                    tint = Color(0xFF4A6374),
                    modifier = Modifier.size(16.dp),
                )
            }
        }

        when {
            error != null -> Text(
                error!!,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
            )
            jobs.isEmpty() -> EmptyContent(
                title = "No active jobs",
                description = "Crawl, ingest, and embed jobs will appear here",
                icon = Icons.Rounded.Work,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp),
            )
            else -> LazyColumn(
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                items(jobs, key = { it.id }) { job ->
                    JobsOverviewItem(job = job)
                }
            }
        }
    }
}
