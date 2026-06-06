package com.axon.app.ui.knowledge

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.knowledge.sections.DomainsSection
import com.axon.app.ui.knowledge.sections.SourcesSection
import com.axon.app.ui.knowledge.sections.StatsSection
import com.axon.app.ui.knowledge.sections.SuggestSection
import kotlinx.collections.immutable.persistentListOf
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraTabs

private val TABS = persistentListOf("Suggest", "Sources", "Domains", "Stats")

enum class KnowledgeTab(val title: String) {
    Suggest("Suggest"),
    Sources("Sources"),
    Domains("Domains"),
    Stats("Stats"),
}

/**
 * Knowledge page — four-tab read-only view over /v1/suggest, /v1/sources,
 * /v1/domains, /v1/stats. Tab selection is `rememberSaveable` so config-change
 * (rotation) restores the user's place. Per-section state lives in
 * [KnowledgeViewModel] with R11 30s memoization, so tab-switching is cheap.
 */
@Composable
fun KnowledgeScreen(
    initialTab: KnowledgeTab = KnowledgeTab.Suggest,
    showChrome: Boolean = true,
    vm: KnowledgeViewModel = viewModel(),
) {
    var selected by rememberSaveable(initialTab) { mutableIntStateOf(initialTab.ordinal) }

    Column(
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (showChrome) {
            Text("Knowledge", style = MaterialTheme.typography.headlineMedium)
            AuroraSeparator()

            AuroraTabs(
                tabs = TABS,
                selectedIndex = selected,
                onTabSelected = { selected = it },
            )
        }

        when (selected) {
            0 -> SuggestSection(vm)
            1 -> SourcesSection(vm)
            2 -> DomainsSection(vm)
            3 -> StatsSection(vm)
        }
    }
}
