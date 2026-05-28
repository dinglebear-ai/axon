package com.axon.app.ui.tools

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraSeparator

@Composable
fun ScrapeTab(vm: ToolsViewModel) {
    val state by vm.scrapeState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        ToolUrlForm(
            buttonLabel = "Scrape",
            submitEnabled = state !is ScrapeUiState.Loading,
            onSubmit = { vm.scrape(it) },
            actionLeft = com.axon.app.ui.operations.modeOptionsCog(),
        )

        when (val s = state) {
            is ScrapeUiState.Loading -> LoadingContent(
                label = "Scraping page…",
                modifier = Modifier.weight(1f),
            )

            is ScrapeUiState.Success -> {
                AuroraCard(
                    modifier = Modifier.fillMaxWidth().weight(1f),
                    variant = AuroraCardVariant.Outlined,
                ) {
                    Column(
                        modifier = Modifier
                            .padding(12.dp)
                            .verticalScroll(rememberScrollState()),
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                s.result.url,
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.weight(1f),
                                maxLines = 1,
                            )
                            val wordCount = s.result.markdown.split(Regex("\\s+")).size
                            Text(
                                "$wordCount words",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        AuroraSeparator(modifier = Modifier.padding(vertical = 8.dp))
                        SelectionContainer {
                            Text(
                                s.result.markdown,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                }
            }

            is ScrapeUiState.Error -> {
                ErrorContent(message = s.message)
                Spacer(Modifier.weight(1f))
            }

            is ScrapeUiState.Idle -> Spacer(Modifier.weight(1f))
        }
    }
}
