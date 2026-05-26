package com.axon.app.ui.tools

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.collections.immutable.persistentListOf
import tv.tootie.aurora.components.AuroraTabs

private val TAB_LABELS = persistentListOf("Scrape", "Map", "Crawl", "Research")

@Composable
fun ToolsScreen(vm: ToolsViewModel = viewModel()) {
    var selectedTab by remember { mutableIntStateOf(0) }

    Column(modifier = Modifier.fillMaxSize()) {
        AuroraTabs(
            tabs = TAB_LABELS,
            selectedIndex = selectedTab,
            onTabSelected = { selectedTab = it },
        )

        when (selectedTab) {
            0 -> ScrapeTab(vm)
            1 -> MapTab(vm)
            2 -> CrawlTab(vm)
            3 -> ResearchTab(vm)
        }
    }
}
