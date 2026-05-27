package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.stringPreferencesKey
import tv.tootie.aurora.components.AuroraSelect
import tv.tootie.aurora.components.AuroraTextField

internal object SummarizeFormKeys {
    val RENDER_MODE      = stringPreferencesKey("mode_options.summarize.render_mode")
    val ROOT_SELECTOR    = stringPreferencesKey("mode_options.summarize.root_selector")
    val EXCLUDE_SELECTOR = stringPreferencesKey("mode_options.summarize.exclude_selector")
    val ALL: List<Preferences.Key<*>> = listOf(RENDER_MODE, ROOT_SELECTOR, EXCLUDE_SELECTOR)
}

private const val DEFAULT_RENDER_MODE = "auto-switch"
private val RENDER_MODES = listOf("http", "chrome", "auto-switch")

@Composable
fun SummarizeOptionsForm() {
    val repo = rememberModeOptionsRepository()
    var renderMode by rememberPersistedState(SummarizeFormKeys.RENDER_MODE, DEFAULT_RENDER_MODE, repo)
    var rootSelector by rememberPersistedState(SummarizeFormKeys.ROOT_SELECTOR, "", repo)
    var excludeSelector by rememberPersistedState(SummarizeFormKeys.EXCLUDE_SELECTOR, "", repo)

    ModeOptionsFormScaffold(
        title = "Summarize options",
        description = "DOM selectors and rendering mode for /v1/summarize.",
        resetKeys = SummarizeFormKeys.ALL,
        repo = repo,
    ) {
        AuroraSelect(
            selectedOption = renderMode,
            onOptionSelected = { renderMode = it },
            options = RENDER_MODES,
            label = "Render mode",
            modifier = Modifier.fillMaxWidth(),
        )
        AuroraTextField(
            value = rootSelector,
            onValueChange = { rootSelector = it },
            label = "Root selector (CSS, optional)",
            modifier = Modifier.fillMaxWidth(),
        )
        AuroraTextField(
            value = excludeSelector,
            onValueChange = { excludeSelector = it },
            label = "Exclude selector (CSS, optional)",
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
