package com.axon.app.ui.options.forms

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.intPreferencesKey

internal object MapFormKeys {
    val LIMIT  = intPreferencesKey("mode_options.map.limit")
    val OFFSET = intPreferencesKey("mode_options.map.offset")
    val ALL: List<Preferences.Key<*>> = listOf(LIMIT, OFFSET)
}

private const val DEFAULT_LIMIT = 10
private const val DEFAULT_OFFSET = 0

@Composable
fun MapOptionsForm() {
    val repo = rememberModeOptionsRepository()
    var limit by rememberPersistedState(MapFormKeys.LIMIT, DEFAULT_LIMIT, repo)
    var offset by rememberPersistedState(MapFormKeys.OFFSET, DEFAULT_OFFSET, repo)

    ModeOptionsFormScaffold(
        title = "Map options",
        description = "Discover URLs at a site without scraping.",
        resetKeys = MapFormKeys.ALL,
        repo = repo,
    ) {
        IntField("Limit", limit) { limit = it }
        IntField("Offset", offset) { offset = it }
    }
}
