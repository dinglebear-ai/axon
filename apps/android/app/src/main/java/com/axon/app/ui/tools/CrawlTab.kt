package com.axon.app.ui.tools

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
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
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant

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
        OutlinedTextField(
            value = urlInput,
            onValueChange = { urlInput = it },
            label = { Text("Start URL") },
            placeholder = { Text("https://example.com") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )

        OutlinedTextField(
            value = maxPagesInput,
            onValueChange = { maxPagesInput = it.filter { c -> c.isDigit() } },
            label = { Text("Max Pages (optional)") },
            placeholder = { Text("e.g. 50") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.fillMaxWidth(),
        )

        Button(
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
                        Text(
                            "Job submitted",
                            style = MaterialTheme.typography.titleSmall,
                            color = MaterialTheme.colorScheme.primary,
                        )
                        Text(
                            "ID: ${s.jobId}",
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
                Spacer(Modifier.height(4.dp))
                OutlinedButton(
                    onClick = { vm.pollCrawlStatus(s.jobId) },
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
                        Text(
                            "Status: ${s.status}",
                            style = MaterialTheme.typography.titleSmall,
                        )
                        Text(
                            "Job ID: ${s.jobId}",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                Spacer(Modifier.height(4.dp))
                OutlinedButton(
                    onClick = { vm.pollCrawlStatus(s.jobId) },
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
