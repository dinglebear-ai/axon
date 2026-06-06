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
    val crawlCount = jobs.count { it.kind == AxonClient.JobKind.Crawl }
    val embedCount = jobs.count { it.kind == AxonClient.JobKind.Embed }
    val ingestCount = jobs.count { it.kind == AxonClient.JobKind.Ingest }
    val extractCount = jobs.count { it.kind == AxonClient.JobKind.Extract }
    val enabledWatchCount = watches.count { it.enabled }

    Column(modifier = Modifier.fillMaxWidth()) {
        if (selectedLevel != null) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 14.dp, end = 4.dp, top = 8.dp, bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = { selectedLevel = null }, modifier = Modifier.size(32.dp)) {
                    Icon(
                        Icons.AutoMirrored.Rounded.ArrowBack,
                        contentDescription = "Back to job queues",
                        tint = Color(0xFF4A6374),
                        modifier = Modifier.size(16.dp),
                    )
                }
                Text(
                    selectedLevel?.label ?: "Jobs",
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
        }

        when {
            selectedLevel == null -> Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                DrawerSubItem(
                    icon = Icons.Rounded.Work,
                    label = "Crawls",
                    detail = "$crawlCount active",
                    onClick = { selectedLevel = JobsDrawerLevel.Crawl },
                )
                DrawerSubItem(
                    icon = Icons.Rounded.Storage,
                    label = "Embeddings",
                    detail = "$embedCount active",
                    onClick = { selectedLevel = JobsDrawerLevel.Embed },
                )
                DrawerSubItem(
                    icon = Icons.Rounded.CloudDownload,
                    label = "Ingestions",
                    detail = "$ingestCount active",
                    onClick = { selectedLevel = JobsDrawerLevel.Ingest },
                )
                DrawerSubItem(
                    icon = Icons.Rounded.DataObject,
                    label = "Extractions",
                    detail = "$extractCount active",
                    onClick = { selectedLevel = JobsDrawerLevel.Extract },
                )
                DrawerSubItem(
                    icon = Icons.Rounded.Schedule,
                    label = "Watches",
                    detail = "$enabledWatchCount enabled",
                    onClick = { selectedLevel = JobsDrawerLevel.Watches },
                )
            }
            error != null -> Text(
                error!!,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
            )
            selectedLevel == JobsDrawerLevel.Watches -> LazyColumn(
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                if (watches.isEmpty()) {
                    item {
                        EmptyContent(
                            title = "No watches",
                            description = "Live watch definitions will appear here",
                            icon = Icons.Rounded.Schedule,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(16.dp),
                        )
                    }
                } else {
                    items(watches, key = { it.id }) { watch ->
                        DrawerSubItem(
                            icon = Icons.Rounded.Schedule,
                            label = watch.name,
                            detail = if (watch.enabled) "Every ${watch.everySeconds}s" else "Paused",
                        )
                    }
                }
            }
            else -> LazyColumn(
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                val kind = when (selectedLevel) {
                    JobsDrawerLevel.Crawl -> AxonClient.JobKind.Crawl
                    JobsDrawerLevel.Embed -> AxonClient.JobKind.Embed
                    JobsDrawerLevel.Ingest -> AxonClient.JobKind.Ingest
                    JobsDrawerLevel.Extract -> AxonClient.JobKind.Extract
                    else -> null
                }
                val filtered = jobs.filter { it.kind == kind }
                if (filtered.isEmpty()) {
                    item {
                        EmptyContent(
                            title = "No active ${selectedLevel?.label?.lowercase()}",
                            description = "Live jobs for this queue will appear here",
                            icon = Icons.Rounded.Work,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(16.dp),
                        )
                    }
                } else {
                    items(filtered, key = { it.id }) { job ->
                        JobsOverviewItem(job = job)
                    }
                }
            }
        }
    }
}
