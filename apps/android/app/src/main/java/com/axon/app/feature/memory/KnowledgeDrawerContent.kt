package com.axon.app.feature.memory

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.BarChart
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.Public
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.DrawerSubItem

@Composable
fun KnowledgeDrawerContent(
    onOpenSuggest: () -> Unit,
    onOpenSources: () -> Unit,
    onOpenDomains: () -> Unit,
    onOpenStats: () -> Unit,
    vm: KnowledgeViewModel = viewModel(),
) {
    val suggest by vm.suggest.collectAsStateWithLifecycle()
    val sources by vm.sources.collectAsStateWithLifecycle()
    val domains by vm.domains.collectAsStateWithLifecycle()
    val stats by vm.stats.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) {
        vm.loadSuggest(focus = null)
        vm.loadSources()
        vm.loadDomains()
        vm.loadStats()
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 6.dp, vertical = 4.dp),
        verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(6.dp),
    ) {
        DrawerSubItem(Icons.Rounded.AutoAwesome, "Suggest", suggestDetail(suggest), onClick = onOpenSuggest)
        DrawerSubItem(Icons.Rounded.Folder, "Sources", sourcesDetail(sources), onClick = onOpenSources)
        DrawerSubItem(Icons.Rounded.Public, "Domains", domainsDetail(domains), onClick = onOpenDomains)
        DrawerSubItem(Icons.Rounded.BarChart, "Stats", statsDetail(stats), onClick = onOpenStats)
    }
}
