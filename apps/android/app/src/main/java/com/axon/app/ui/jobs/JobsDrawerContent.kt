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
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.CloudDownload
import androidx.compose.material.icons.rounded.DataObject
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material.icons.rounded.Work
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.remote.AxonClient
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.DrawerSubItem

private enum class JobsDrawerLevel(val label: String) {
    Crawl("Crawls"),
    Embed("Embeddings"),
    Ingest("Ingestions"),
    Extract("Extractions"),
    Watches("Watches"),
}

@Composable
fun JobsDrawerContent(vm: JobsOverviewViewModel = viewModel()) {
    val jobs by vm.activeJobs.collectAsStateWithLifecycle()
    val watches by vm.watches.collectAsStateWithLifecycle()
    val error by vm.errorMessage.collectAsStateWithLifecycle()
    var selectedLevel by remember { mutableStateOf<JobsDrawerLevel?>(null) }

    Column(modifier = Modifier.fillMaxWidth()) {
        // Header row with refresh button
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 14.dp, end = 4.dp, top = 8.dp, bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                selectedLevel?.label ?: "Jobs",
                style = MaterialTheme.typography.labelMedium,
                color = Color(0xFFA7BCC9),
                modifier = Modifier.weight(1f),
            )
            if (selectedLevel != null) {
                IconButton(onClick = { selectedLevel = null }, modifier = Modifier.size(32.dp)) {
                    Icon(
                        Icons.AutoMirrored.Rounded.ArrowBack,
                        contentDescription = "Back to job queues",
                        tint = Color(0xFF4A6374),
                        modifier = Modifier.size(16.dp),
                    )
                }
            }
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
            selectedLevel == null -> Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 4.dp),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                DrawerSubItem(
                    icon = Icons.Rounded.Work,
                    label = "Crawls",
                    detail = "${jobs.count { it.kind == AxonClient.JobKind.Crawl }} active",
                    onClick = { selectedLevel = JobsDrawerLevel.Crawl },
                )
                DrawerSubItem(
                    icon = Icons.Rounded.Storage,
                    label = "Embeddings",
                    detail = "${jobs.count { it.kind == AxonClient.JobKind.Embed }} active",
                    onClick = { selectedLevel = JobsDrawerLevel.Embed },
                )
                DrawerSubItem(
                    icon = Icons.Rounded.CloudDownload,
                    label = "Ingestions",
                    detail = "${jobs.count { it.kind == AxonClient.JobKind.Ingest }} active",
                    onClick = { selectedLevel = JobsDrawerLevel.Ingest },
                )
                DrawerSubItem(
                    icon = Icons.Rounded.DataObject,
                    label = "Extractions",
                    detail = "${jobs.count { it.kind == AxonClient.JobKind.Extract }} active",
                    onClick = { selectedLevel = JobsDrawerLevel.Extract },
                )
                DrawerSubItem(
                    icon = Icons.Rounded.Schedule,
                    label = "Watches",
                    detail = "${watches.count { it.enabled }} enabled",
                    onClick = { selectedLevel = JobsDrawerLevel.Watches },
                )
            }
            selectedLevel == JobsDrawerLevel.Watches -> {
                if (watches.isEmpty()) {
                    EmptyContent(
                        title = "No watches",
                        description = "Scheduled watches will appear here",
                        icon = Icons.Rounded.Schedule,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                    )
                } else {
                    LazyColumn(
                        modifier = Modifier.fillMaxWidth(),
                        verticalArrangement = Arrangement.spacedBy(2.dp),
                    ) {
                        items(watches, key = { it.id }) { watch ->
                            DrawerSubItem(
                                icon = Icons.Rounded.Schedule,
                                label = watch.name,
                                detail = if (watch.enabled) "Every ${watch.everySeconds}s" else "Paused",
                            )
                        }
                    }
                }
            }
            else -> {
                val kind = when (selectedLevel) {
                    JobsDrawerLevel.Crawl -> AxonClient.JobKind.Crawl
                    JobsDrawerLevel.Embed -> AxonClient.JobKind.Embed
                    JobsDrawerLevel.Ingest -> AxonClient.JobKind.Ingest
                    JobsDrawerLevel.Extract -> AxonClient.JobKind.Extract
                    else -> null
                }
                val filteredJobs = jobs.filter { it.kind == kind }
                if (filteredJobs.isEmpty()) {
                    EmptyContent(
                        title = "No active jobs",
                        description = "Active ${selectedLevel?.label?.lowercase()} will appear here",
                        icon = Icons.Rounded.Work,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                    )
                } else {
                    LazyColumn(
                        modifier = Modifier.fillMaxWidth(),
                        verticalArrangement = Arrangement.spacedBy(2.dp),
                    ) {
                        items(filteredJobs, key = { it.id }) { job ->
                            JobsOverviewItem(job = job)
                        }
                    }
                }
            }
        }
    }
}
