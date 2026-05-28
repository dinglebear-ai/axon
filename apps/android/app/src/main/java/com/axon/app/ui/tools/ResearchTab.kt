package com.axon.app.ui.tools

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import android.net.Uri
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.data.remote.ResearchHit
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraItem
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraTextField

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
        AuroraTextField(
            value = queryInput,
            onValueChange = { queryInput = it },
            label = "Research Query",
            placeholder = "What is…",
            singleLine = false,
            modifier = Modifier.fillMaxWidth(),
        )

        androidx.compose.foundation.layout.Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            com.axon.app.ui.operations.modeOptionsCog()?.invoke()
            AuroraButton(
                onClick = { vm.research(queryInput.trim()) },
                enabled = state !is ResearchUiState.Loading,
                modifier = Modifier.weight(1f),
            ) {
                Text("Research")
            }
        }

        when (val s = state) {
            is ResearchUiState.Loading -> LoadingContent(
                label = "Researching… (may take up to 2 minutes)",
                modifier = Modifier.weight(1f),
            )

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
                                    AuroraSeparator()
                                    Text(summary, style = MaterialTheme.typography.bodyMedium)
                                }
                            }
                        }
                    }

                    if (s.result.hits.isNotEmpty()) {
                        item(key = "hits_header") {
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.SpaceBetween,
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    "Search Results",
                                    style = MaterialTheme.typography.labelLarge,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                                Text(
                                    "${s.result.hits.size}",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                        items(s.result.hits, key = { "${it.position}_${it.url}" }) { hit ->
                            ResearchHitCard(hit)
                        }
                    }
                }
            }

            is ResearchUiState.Error -> {
                ErrorContent(message = s.message)
                Spacer(Modifier.weight(1f))
            }

            is ResearchUiState.Idle -> Spacer(Modifier.weight(1f))
        }
    }
}

@Composable
private fun ResearchHitCard(hit: ResearchHit) {
    val uriHandler = LocalUriHandler.current
    AuroraItem(
        title = hit.title,
        description = hit.snippet?.take(120)?.let { if (it.length == 120) "$it…" else it },
        leadingContent = {
            Text(
                "#${hit.position}",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
        trailingContent = {
            Text(
                runCatching { java.net.URI(hit.url).host?.removePrefix("www.") ?: "" }
                    .getOrDefault(""),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.primary,
            )
        },
        onClick = {
            val scheme = Uri.parse(hit.url).scheme
            if (scheme == "https" || scheme == "http") {
                runCatching { uriHandler.openUri(hit.url) }
            }
        },
    )
}
