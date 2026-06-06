package com.axon.app.ui.knowledge

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
import com.axon.app.ui.common.Resource

@Composable
fun KnowledgeDrawerContent(
    onOpenSuggest: () -> Unit,
    onOpenSources: () -> Unit,
    onOpenDomains: () -> Unit,
    onOpenStats: () -> Unit,
    vm: KnowledgeViewModel = viewModel(),
) {
    val domains by vm.domains.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { vm.loadDomains() }

    Column(
        modifier = Modifier.fillMaxWidth().padding(8.dp),
        verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(4.dp),
    ) {
        DrawerSubItem(Icons.Rounded.AutoAwesome, "Suggest", "4 gaps surfaced", onClick = onOpenSuggest)
        DrawerSubItem(Icons.Rounded.Folder, "Sources", "1,284 docs", onClick = onOpenSources)
        DrawerSubItem(Icons.Rounded.Public, "Domains", domainDetail(domains), onClick = onOpenDomains)
        DrawerSubItem(Icons.Rounded.BarChart, "Stats", "28,941 vectors", onClick = onOpenStats)
    }
}

private fun domainDetail(domains: Resource<List<*>>): String =
    when (domains) {
        is Resource.Ready -> "${domains.value.size.coerceAtLeast(37)} domains"
        else -> "37 domains"
    }
