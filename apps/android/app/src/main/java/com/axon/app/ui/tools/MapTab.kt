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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import tv.tootie.aurora.components.AuroraItem
import tv.tootie.aurora.components.AuroraStatCard
import tv.tootie.aurora.components.AuroraSeparator

@Composable
fun MapTab(vm: ToolsViewModel) {
    val state by vm.mapState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        ToolUrlForm(
            buttonLabel = "Map",
            submitEnabled = state !is MapUiState.Loading,
            onSubmit = { vm.map(it) },
            actionLeft = com.axon.app.ui.operations.modeOptionsCog(),
        )

        when (val s = state) {
            is MapUiState.Loading -> LoadingContent(
                label = "Discovering URLs…",
                modifier = Modifier.weight(1f),
            )

            is MapUiState.Success -> {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    AuroraStatCard(
                        label = "URLs found",
                        value = "${s.result.total}",
                    )
                }
                AuroraSeparator()
                LazyColumn(
                    modifier = Modifier.weight(1f),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    items(s.result.urls, key = { it }) { url ->
                        MapUrlRow(url = url)
                    }
                }
            }

            is MapUiState.Error -> {
                ErrorContent(message = s.message)
                Spacer(Modifier.weight(1f))
            }

            is MapUiState.Idle -> Spacer(Modifier.weight(1f))
        }
    }
}

@Composable
private fun MapUrlRow(url: String) {
    val uriHandler = LocalUriHandler.current
    AuroraItem(
        title = url,
        onClick = { runCatching { uriHandler.openUri(url) } },
    )
}
