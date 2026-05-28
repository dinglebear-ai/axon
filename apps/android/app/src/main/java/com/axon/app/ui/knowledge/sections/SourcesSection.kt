package com.axon.app.ui.knowledge.sections

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Storage
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.data.repository.SourceEntryUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.knowledge.KnowledgeViewModel
import com.axon.app.ui.nav.LocalOpenDocument
import tv.tootie.aurora.components.AuroraItem

@Composable
fun SourcesSection(vm: KnowledgeViewModel) {
    val state by vm.sources.collectAsStateWithLifecycle()
    val openDoc = LocalOpenDocument.current

    LaunchedEffect(Unit) { vm.loadSources() }

    when (val s = state) {
        Resource.Idle, Resource.Loading -> LoadingContent(
            label = "Loading sources…",
            modifier = Modifier.fillMaxWidth(),
        )
        is Resource.Error -> ErrorContent(message = s.message, onRetry = { vm.loadSources(force = true) })
        is Resource.Ready -> {
            val entries = s.value
            if (entries.isEmpty()) {
                EmptyContent(
                    title = "No sources indexed",
                    description = "Use Ask, Search, or Ingest to populate the knowledge base.",
                    icon = Icons.Outlined.Storage,
                    modifier = Modifier.fillMaxWidth(),
                )
            } else {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    items(entries, key = { it.url }) { entry ->
                        AuroraItem(
                            title = entry.url,
                            description = "${entry.chunks} chunks",
                            trailingContent = {
                                Text(
                                    "${entry.chunks}",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            },
                            onClick = { openDoc(entry.url) },
                        )
                    }
                }
            }
        }
    }
}

