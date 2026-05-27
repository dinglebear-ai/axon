package com.axon.app.ui.operations

import android.widget.Toast
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.ask.AskScreen
import com.axon.app.ui.nav.LocalModeOptionsCog
import com.axon.app.ui.query.QueryScreen
import com.axon.app.ui.tools.CrawlTab
import com.axon.app.ui.tools.MapTab
import com.axon.app.ui.tools.ResearchTab
import com.axon.app.ui.tools.ScrapeTab
import com.axon.app.ui.tools.ToolsViewModel

/**
 * The Operations page. The body is the active mode's screen; the FAB is a
 * draggable, floating button that triggers the mode picker. Each underlying
 * screen renders its own `AuroraPromptInput` — they read [LocalModeOptionsCog]
 * to surface an inline cog left of the Send button.
 */
@Composable
fun OperationsScreen(vm: OperationsViewModel = viewModel()) {
    val activeMode by vm.activeMode.collectAsStateWithLifecycle()
    var sheetVisible by remember { mutableStateOf(false) }
    val toolsVm: ToolsViewModel = viewModel()
    val context = LocalContext.current

    // Mode-options cog handler — provided to every prompt input via CompositionLocal.
    // Wired to a Toast until the per-mode flag-form screen is built (axon_rust-ivjr).
    val onModeOptions = remember(activeMode) {
        {
            Toast.makeText(
                context,
                "${activeMode.label} options — not yet wired",
                Toast.LENGTH_SHORT,
            ).show()
        }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        CompositionLocalProvider(LocalModeOptionsCog provides onModeOptions) {
            when (activeMode) {
                OperationMode.Ask      -> AskScreen()
                OperationMode.Query    -> QueryScreen()
                OperationMode.Scrape   -> ScrapeTab(toolsVm)
                OperationMode.Crawl    -> CrawlTab(toolsVm)
                OperationMode.Map      -> MapTab(toolsVm)
                OperationMode.Research -> ResearchTab(toolsVm)
                OperationMode.Summarize,
                OperationMode.Ingest,
                OperationMode.Search   -> StubModeForm(mode = activeMode)
            }
        }

        DraggableFab(
            onClick = { sheetVisible = true },
            content = { Icon(activeMode.icon, contentDescription = activeMode.label) },
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
