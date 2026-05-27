package com.axon.app.ui.document

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone

/**
 * Renders the full assembled document for a URL. Triggered when a [QueryHitCard]
 * is tapped; loads via [DocumentViewModel.load] which calls /v1/retrieve.
 */
@Composable
fun DocumentScreen(
    url: String,
    vm: DocumentViewModel = viewModel(),
) {
    LaunchedEffect(url) { vm.load(url) }
    val state by vm.uiState.collectAsStateWithLifecycle()
    val uriHandler = LocalUriHandler.current

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        when (val s = state) {
            is DocumentUiState.Loading -> LoadingContent(
                label = "Fetching document…",
                modifier = Modifier.fillMaxWidth(),
            )

            is DocumentUiState.Error -> ErrorContent(message = s.message)

            is DocumentUiState.Success -> {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        s.result.matchedUrl ?: s.result.requestedUrl,
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.weight(1f).padding(end = 8.dp),
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    AuroraStatusIndicator(
                        tone = AuroraStatusTone.Online,
                        label = "${s.result.chunkCount} chunks",
                    )
                }
                AuroraButton(
                    onClick = { runCatching { uriHandler.openUri(s.result.matchedUrl ?: s.result.requestedUrl) } },
                    variant = AuroraButtonVariant.Outlined,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Open source URL in browser") }

                if (s.result.truncated) {
                    AuroraCallout(
                        title = "Truncated",
                        message = "Document was truncated to fit the token budget; some chunks may be missing.",
                        variant = AuroraCalloutVariant.Warn,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                s.result.warnings.forEach { w ->
                    AuroraCallout(
                        title = "Warning",
                        message = w,
                        variant = AuroraCalloutVariant.Warn,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }

                AuroraSeparator()

                AuroraCard(
                    modifier = Modifier.fillMaxWidth().weight(1f),
                    variant = AuroraCardVariant.Outlined,
                ) {
                    Column(
                        modifier = Modifier
                            .padding(12.dp)
                            .verticalScroll(rememberScrollState()),
                    ) {
                        SelectionContainer {
                            Text(s.result.content, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
        }
    }
}
