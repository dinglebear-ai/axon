package com.axon.app.ui.operations

import androidx.compose.runtime.Composable
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.ask.AskScreen
import com.axon.app.ui.query.QueryScreen
import com.axon.app.ui.tools.CrawlTab
import com.axon.app.ui.tools.MapTab
import com.axon.app.ui.tools.ResearchTab
import com.axon.app.ui.tools.ScrapeTab
import com.axon.app.ui.tools.ToolsViewModel

/**
 * Dispatch table from [OperationMode] to the content composable. Extracted from
 * OperationsScreen so each feature task can swap one row independently (one-line
 * additive change) instead of conflict-prone edits inside the screen scaffold.
 *
 * Wave-2 task 3 (Summarize), wave-3 task 7 (Search), and wave-4 task 8 (Ingest)
 * each replace one TODO branch in the `when` below with their real screen.
 */
@Composable
fun ModeContentHost(activeMode: OperationMode) {
    val toolsVm: ToolsViewModel = viewModel()
    when (activeMode) {
        OperationMode.Ask       -> AskScreen()
        OperationMode.Query     -> QueryScreen()
        OperationMode.Scrape    -> ScrapeTab(toolsVm)
        OperationMode.Crawl     -> CrawlTab(toolsVm)
        OperationMode.Map       -> MapTab(toolsVm)
        OperationMode.Research  -> ResearchTab(toolsVm)
        OperationMode.Summarize -> com.axon.app.ui.summarize.SummarizeScreen()
        OperationMode.Search    -> StubModeForm(mode = activeMode)  // replaced in Task 7
        OperationMode.Ingest    -> StubModeForm(mode = activeMode)  // replaced in Task 8
    }
}
