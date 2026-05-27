package com.axon.app.ui.knowledge.sections

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Dns
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.data.repository.DomainFacetUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.knowledge.KnowledgeViewModel
import tv.tootie.aurora.components.AuroraItem

@Composable
fun DomainsSection(vm: KnowledgeViewModel) {
    val state by vm.domains.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { vm.loadDomains(limit = 200) }

    when (val s = state) {
        Resource.Idle, Resource.Loading -> LoadingContent(
            label = "Loading domains…",
            modifier = Modifier.fillMaxWidth(),
        )
        is Resource.Error -> ErrorContent(message = s.message, onRetry = { vm.loadDomains(force = true) })
        is Resource.Ready -> {
            val facets = s.value
            if (facets.isEmpty()) {
                EmptyContent(
                    title = "No domains indexed",
                    description = "Index a few sources to populate domain facets.",
                    icon = Icons.Outlined.Dns,
                    modifier = Modifier.fillMaxWidth(),
                )
            } else {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    items(facets, key = { it.domain }) { facet ->
                        AuroraItem(
                            title = facet.domain,
                            description = "${facet.vectors} vectors",
                            trailingContent = {
                                Text(
                                    "${facet.vectors}",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            },
                        )
                    }
                }
            }
        }
    }
}
