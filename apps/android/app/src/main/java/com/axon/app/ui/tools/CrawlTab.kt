package com.axon.app.ui.tools

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraKbd
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone
import tv.tootie.aurora.components.AuroraTextField

@Composable
fun CrawlTab(vm: ToolsViewModel) {
    val state by vm.crawlState.collectAsStateWithLifecycle()
    var urlInput by remember { mutableStateOf("") }
    var maxPagesInput by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        AuroraTextField(
            value = urlInput,
            onValueChange = { urlInput = it },
            label = "Start URL",
            placeholder = "https://example.com",
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )

        AuroraTextField(
            value = maxPagesInput,
            onValueChange = { maxPagesInput = it.filter { c -> c.isDigit() } },
            label = "Max Pages (optional)",
            placeholder = "e.g. 50",
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.fillMaxWidth(),
        )

        AuroraButton(
            onClick = {
                val maxPages = maxPagesInput.toIntOrNull()
                vm.crawl(urlInput.trim(), maxPages)
            },
            enabled = state !is CrawlUiState.Loading,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Start Crawl")
        }

        when (val s = state) {
            is CrawlUiState.Loading -> LoadingContent(
                label = "Submitting crawl job…",
                modifier = Modifier.weight(1f),
            )

            is CrawlUiState.Submitted -> {
                AuroraCard(
                    modifier = Modifier.fillMaxWidth(),
                    variant = AuroraCardVariant.Filled,
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        AuroraStatusIndicator(
                            tone = AuroraStatusTone.Queued,
                            label = "Job submitted",
                        )
                        AuroraSeparator()
                        Text(
                            "Job ID",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        AuroraKbd(key = s.jobId, contentDescription = "Job ID ${s.jobId}")
                    }
                }
                Spacer(Modifier.height(4.dp))
                AuroraButton(
                    onClick = { vm.pollCrawlStatus(s.jobId) },
                    variant = AuroraButtonVariant.Outlined,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("Check Status")
                }
                Spacer(Modifier.weight(1f))
            }

            is CrawlUiState.StatusPolled -> {
                AuroraCard(
                    modifier = Modifier.fillMaxWidth(),
                    variant = AuroraCardVariant.Filled,
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        val tone = when (s.status.lowercase()) {
                            "completed", "done" -> AuroraStatusTone.Online
                            "running", "processing" -> AuroraStatusTone.Syncing
                            "failed", "error" -> AuroraStatusTone.Error
                            "pending", "queued" -> AuroraStatusTone.Queued
                            else -> AuroraStatusTone.Queued
                        }
                        AuroraStatusIndicator(tone = tone, label = s.status)
                        AuroraSeparator()
                        Text(
                            "Job ID",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        AuroraKbd(key = s.jobId, contentDescription = "Job ID ${s.jobId}")
                        s.pagesCrawled?.let { pages ->
                            Text(
                                "Pages crawled: $pages",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        s.serverError?.let { errMsg ->
                            Text(
                                "Server error: $errMsg",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.error,
                            )
                        }
                    }
                }
                Spacer(Modifier.height(4.dp))
                AuroraButton(
                    onClick = { vm.pollCrawlStatus(s.jobId) },
                    variant = AuroraButtonVariant.Outlined,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("Refresh Status")
                }
                Spacer(Modifier.weight(1f))
            }

            is CrawlUiState.Error -> {
                ErrorContent(message = s.message)
                Spacer(Modifier.weight(1f))
            }

            is CrawlUiState.Idle -> Spacer(Modifier.weight(1f))
        }
    }
}
