package com.axon.app.ui.searchweb

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Public
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.SearchWebResultUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.nav.LocalOpenDocument
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraPromptInput
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone

/**
 * Search mode screen: live web search via Tavily, results auto-indexed server-side.
 *
 * R16 — when the server reports it skipped enqueueing some result-driven crawls
 * (`crawlJobsSkipped > 0`) AND we still have results to show, surface a Warn
 * callout so the user knows indexing is degraded (queue cap likely hit).
 */
@Composable
fun SearchWebScreen(vm: SearchWebViewModel = viewModel()) {
    val state by vm.uiState.collectAsStateWithLifecycle()
    val openDoc = LocalOpenDocument.current
    var input by remember { mutableStateOf("") }

    Column(
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Search the web", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()

        when (val s = state) {
            Resource.Idle -> EmptyContent(
                title = "Search the live web",
                description = "Results are auto-indexed into your knowledge base.",
                icon = Icons.Outlined.Public,
                modifier = Modifier.fillMaxWidth(),
            )
            Resource.Loading -> LoadingContent(
                label = "Searching…",
                modifier = Modifier.fillMaxWidth(),
            )
            is Resource.Error -> ErrorContent(message = s.message)
            is Resource.Ready -> {
                val result = s.value
                Column(
                    modifier = Modifier.fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    AuroraStatusIndicator(
                        tone = AuroraStatusTone.Queued,
                        label = "${result.crawlJobsEnqueued} crawl jobs enqueued",
                    )
                    // R16 — auto-crawl backpressure callout.
                    if (result.crawlJobsSkipped > 0 && result.results.isNotEmpty()) {
                        AuroraCallout(
                            title = "Auto-crawl queue full",
                            message = "Some results were not enqueued for indexing — try again later.",
                            variant = AuroraCalloutVariant.Warn,
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                    LazyColumn(
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        items(result.results, key = { it.url }) { hit ->
                            AuroraCard(
                                onClick = { openDoc(hit.url) },
                                modifier = Modifier.fillMaxWidth(),
                                variant = AuroraCardVariant.Outlined,
                            ) {
                                Column(
                                    modifier = Modifier.padding(12.dp),
                                    verticalArrangement = Arrangement.spacedBy(4.dp),
                                ) {
                                    Text(
                                        hit.title,
                                        style = MaterialTheme.typography.titleSmall,
                                        maxLines = 2,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                    Text(
                                        hit.url,
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.primary,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                    hit.snippet?.let {
                                        Text(
                                            it,
                                            style = MaterialTheme.typography.bodySmall,
                                            maxLines = 3,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        AuroraSeparator()
        AuroraPromptInput(
            value = input,
            onValueChange = { input = it },
            onSend = {
                vm.submit(input)
                input = ""
            },
            placeholder = "Search the web…",
            loading = state is Resource.Loading,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
