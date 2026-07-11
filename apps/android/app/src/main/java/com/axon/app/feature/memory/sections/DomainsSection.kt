package com.axon.app.feature.memory.sections

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Dns
import androidx.compose.material.icons.rounded.Public
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
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.feature.memory.KnowledgeResultRow
import com.axon.app.feature.memory.KnowledgeViewModel

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
                val reveal = rememberRevealState()
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    itemsIndexed(facets, key = { _, it -> it.domain }) { index, facet ->
                        KnowledgeResultRow(
                            icon = Icons.Rounded.Public,
                            title = facet.domain,
                            detail = "Indexed domain",
                            metric = "${facet.vectors} vectors",
                            modifier = Modifier
                                .animateItem()
                                .revealOnce(reveal, facet.domain, index),
                        )
                    }
                }
            }
        }
    }
}
