package com.axon.app.ui.jobs

import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.remote.AxonClient
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.flow.collect
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraTabs

private val TABS = persistentListOf("Crawl", "Embed", "Extract", "Ingest")

/**
 * Tab-index → JobKind dispatch (R15). NEVER replace with `JobKind.values()[i]` —
 * that couples enum declaration order to this list and silently desyncs the cancel
 * target if either is reordered.
 */
private val tabKinds = listOf(
    AxonClient.JobKind.Crawl,
    AxonClient.JobKind.Embed,
    AxonClient.JobKind.Extract,
    AxonClient.JobKind.Ingest,
)

@Composable
fun JobsScreen(vm: JobsViewModel = viewModel()) {
    var selected by rememberSaveable { mutableIntStateOf(0) }
    val recent by vm.recent.collectAsStateWithLifecycle()
    val jobsState by vm.visibleJobs.collectAsStateWithLifecycle()
    val ctx = LocalContext.current

    // R10: drive the VM's single poll flow off the visible tab.
    LaunchedEffect(selected) {
        vm.selectTab(tabKinds[selected])
    }

    // Surface one-shot messages (cancel failures, status fetch failures) as Toasts.
    LaunchedEffect(vm) {
        vm.messages.collect { msg ->
            Toast.makeText(ctx, msg, Toast.LENGTH_SHORT).show()
        }
    }

    Column(
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Jobs", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()

        AuroraTabs(tabs = TABS, selectedIndex = selected, onTabSelected = { selected = it })

        when (val s = jobsState) {
            Resource.Idle, Resource.Loading -> LoadingContent("Loading jobs…", Modifier.fillMaxWidth())
            is Resource.Error -> ErrorContent(message = s.message)
            is Resource.Ready -> {
                LazyColumn(
                    modifier = Modifier.fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(s.value, key = { it.id }) { job ->
                        JobRow(job = job, onCancel = { vm.cancel(job.id) })
                    }
                }
            }
        }

        if (recent.isNotEmpty()) {
            Text("Recent submissions", style = MaterialTheme.typography.labelLarge)
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                recent.take(3).forEach { r ->
                    Text(
                        text = "${r.kind}: ${r.target} (${r.jobId.take(8)})",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}
