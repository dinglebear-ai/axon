package com.axon.app.ui.operations

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.ask.AskScreen
import com.axon.app.ui.query.QueryScreen
import com.axon.app.ui.tools.CrawlTab
import com.axon.app.ui.tools.MapTab
import com.axon.app.ui.tools.ResearchTab
import com.axon.app.ui.tools.ScrapeTab
import com.axon.app.ui.tools.ToolsViewModel

/**
 * The Operations page: an active-mode-driven host. The FAB opens [ModePickerSheet]
 * to switch [OperationMode]s. The selected mode determines which body composable
 * renders; modes without a wired client method fall back to [StubModeForm].
 *
 * The shared [ToolsViewModel] for scrape/crawl/map/research is reused so existing
 * tab state survives mode switches inside the same activity instance.
 */
@Composable
fun OperationsScreen(vm: OperationsViewModel = viewModel()) {
    val activeMode by vm.activeMode.collectAsStateWithLifecycle()
    var sheetVisible by remember { mutableStateOf(false) }
    val toolsVm: ToolsViewModel = viewModel()

    Box(modifier = Modifier.fillMaxSize()) {
        when (activeMode) {
            OperationMode.Ask     -> AskScreen()
            OperationMode.Query   -> QueryScreen()
            OperationMode.Scrape  -> ScrapeTab(toolsVm)
            OperationMode.Crawl   -> CrawlTab(toolsVm)
            OperationMode.Map     -> MapTab(toolsVm)
            OperationMode.Research -> ResearchTab(toolsVm)
            OperationMode.Summarize,
            OperationMode.Ingest,
            OperationMode.Search  -> StubModeForm(mode = activeMode)
        }

        ExtendedFloatingActionButton(
            onClick = { sheetVisible = true },
            icon = { Icon(activeMode.icon, contentDescription = activeMode.label) },
            text = { Text(activeMode.label) },
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(16.dp),
        )
    }

    if (sheetVisible) {
        ModePickerSheet(
            activeMode = activeMode,
            onSelect = { mode ->
                vm.setMode(mode)
                sheetVisible = false
            },
            onDismiss = { sheetVisible = false },
        )
    }
}
