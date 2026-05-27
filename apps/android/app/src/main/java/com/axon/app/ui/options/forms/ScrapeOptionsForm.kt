package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import tv.tootie.aurora.components.AuroraSelect
import tv.tootie.aurora.components.AuroraTextField

internal object ScrapeFormKeys {
    val RENDER_MODE = stringPreferencesKey("mode_options.scrape.render_mode")
    val FORMAT      = stringPreferencesKey("mode_options.scrape.format")
    val EMBED       = booleanPreferencesKey("mode_options.scrape.embed")
    val COLLECTION  = stringPreferencesKey("mode_options.scrape.collection")
    val ALL: List<Preferences.Key<*>> = listOf(RENDER_MODE, FORMAT, EMBED, COLLECTION)
}

private const val DEFAULT_RENDER_MODE = "auto-switch"
private const val DEFAULT_FORMAT = "markdown"
private const val DEFAULT_EMBED = true
private const val DEFAULT_COLLECTION = "axon"

private val RENDER_MODES = listOf("http", "chrome", "auto-switch")
private val FORMATS = listOf("markdown", "html", "rawHtml", "json")

@Composable
fun ScrapeOptionsForm() {
    val repo = rememberModeOptionsRepository()
    var renderMode by rememberPersistedState(ScrapeFormKeys.RENDER_MODE, DEFAULT_RENDER_MODE, repo)
    var format by rememberPersistedState(ScrapeFormKeys.FORMAT, DEFAULT_FORMAT, repo)
    var embed by rememberPersistedState(ScrapeFormKeys.EMBED, DEFAULT_EMBED, repo)
    var collection by rememberPersistedState(ScrapeFormKeys.COLLECTION, DEFAULT_COLLECTION, repo)

    ModeOptionsFormScaffold(
        title = "Scrape options",
        description = "Single-URL scrape to markdown / HTML.",
        resetKeys = ScrapeFormKeys.ALL,
        repo = repo,
    ) {
        AuroraSelect(
            selectedOption = renderMode,
            onOptionSelected = { renderMode = it },
            options = RENDER_MODES,
            label = "Render mode",
            modifier = Modifier.fillMaxWidth(),
        )
        AuroraSelect(
            selectedOption = format,
            onOptionSelected = { format = it },
            options = FORMATS,
            label = "Format",
            modifier = Modifier.fillMaxWidth(),
        )
        AuroraTextField(
            value = collection,
            onValueChange = { collection = it },
            label = "Collection",
            modifier = Modifier.fillMaxWidth(),
        )
        SwitchRow("Auto-embed scraped content", embed) { embed = it }
    }
}
