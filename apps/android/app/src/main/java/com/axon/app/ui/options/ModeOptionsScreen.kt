package com.axon.app.ui.options

import android.app.Activity
import android.view.WindowManager
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.platform.LocalView
import com.axon.app.ui.operations.OperationMode
import com.axon.app.ui.options.forms.AskOptionsForm
import com.axon.app.ui.options.forms.MapOptionsForm
import com.axon.app.ui.options.forms.QueryOptionsForm
import com.axon.app.ui.options.forms.ResearchOptionsForm
import com.axon.app.ui.options.forms.ScrapeOptionsForm
import com.axon.app.ui.options.forms.SearchWebOptionsForm
import com.axon.app.ui.options.forms.SiteSourceOptionsForm
import com.axon.app.ui.options.forms.SourceOptionsForm
import com.axon.app.ui.options.forms.SummarizeOptionsForm

/**
 * Mode-options screen. Renders the form for [mode] and applies `FLAG_SECURE`
 * to the window for the lifetime of this composition so the screen does NOT
 * appear in the recent-apps screenshot — mitigates R3 risk that sensitive
 * Authorization / Cookie / X-Api-Key header values bleed via task-switcher.
 */
@Composable
fun ModeOptionsScreen(mode: OperationMode) {
    val view = LocalView.current
    DisposableEffect(Unit) {
        val window = (view.context as? Activity)?.window
        window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        onDispose {
            window?.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
    }

    when (mode) {
        OperationMode.Ask -> AskOptionsForm()
        OperationMode.Query -> QueryOptionsForm()
        OperationMode.Summarize -> SummarizeOptionsForm()
        OperationMode.Research -> ResearchOptionsForm()
        OperationMode.Scrape -> ScrapeOptionsForm()
        OperationMode.SourceSite -> SiteSourceOptionsForm()
        OperationMode.Search -> SearchWebOptionsForm()
        OperationMode.Map -> MapOptionsForm()
        OperationMode.Source -> SourceOptionsForm()
    }
}
