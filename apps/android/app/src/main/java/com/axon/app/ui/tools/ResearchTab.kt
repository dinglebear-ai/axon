package com.axon.app.ui.tools

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.data.remote.ResearchHit
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraThinking

@Composable
fun ResearchTab(vm: ToolsViewModel) {
    val state by vm.researchState.collectAsStateWithLifecycle()
    var queryInput by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        OutlinedTextField(
            value = queryInput,
            onValueChange = { queryInput = it },
            label = { Text("Research Query") },
            placeholder = { Text("What is…") },
            minLines = 2,
            modifier = Modifier.fillMaxWidth(),
        )

        Button(
            onClick = { vm.research(queryInput.trim()) },
            enabled = state !is ResearchUiState.Loading,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Research")
        }

        when (val s = state) {
            is ResearchUiState.Loading -> {
                Box(
                    modifier = Modifier.fillMaxWidth().weight(1f),
                    contentAlignment = Alignment.Center,
                ) {
                    AuroraThinking(label = "Researching… (may take up to 30s)")
                }
            }

            is ResearchUiState.Success -> {
                LazyColumn(
                    modifier = Modifier.weight(1f),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    s.result.summary?.let { summary ->
                        item(key = "summary") {
                            AuroraCard(
                                modifier = Modifier.fillMaxWidth(),
                                variant = AuroraCardVariant.Filled,
                            ) {
                                Column(
                                    modifier = Modifier.padding(16.dp),
                                    verticalArrangement = Arrangement.spacedBy(6.dp),
                                ) {
                                    Text(
                                        "Summary",
                                        style = MaterialTheme.typography.labelMedium,
                                        color = MaterialTheme.colorScheme.primary,
                                    )
                                    Text(summary, style = MaterialTheme.typography.bodyMedium)
                                }
                            }
                        }
                    }

                    if (s.result.hits.isNotEmpty()) {
                        item(key = "hits_header") {
                            Text(
                                "Search Results (${s.result.hits.size})",
                                style = MaterialTheme.typography.labelLarge,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        items(s.result.hits, key = { "${it.position}_${it.url}" }) { hit ->
                            ResearchHitCard(hit)
                        }
                    }
                }
            }

            is ResearchUiState.Error -> {
                AuroraCallout(
                    title = "Error",
                    message = s.message,
                    variant = AuroraCalloutVariant.Error,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.weight(1f))
            }

            is ResearchUiState.Idle -> Spacer(Modifier.weight(1f))
        }
    }
}

@Composable
private fun ResearchHitCard(hit: ResearchHit) {
    val uriHandler = LocalUriHandler.current
    AuroraCard(
        onClick = { runCatching { uriHandler.openUri(hit.url) } },
        modifier = Modifier.fillMaxWidth(),
        variant = AuroraCardVariant.Outlined,
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                hit.title,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                hit.url,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.primary,
                maxLines = 1,
            )
            hit.snippet?.let { snippet ->
                Text(
                    snippet,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 3,
                )
            }
        }
    }
}
