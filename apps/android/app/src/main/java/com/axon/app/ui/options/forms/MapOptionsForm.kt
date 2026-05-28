package com.axon.app.ui.options.forms

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import com.axon.app.data.repository.options.MapFormKeys

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
        IntField("Limit", limit) { limit = it.coerceAtLeast(0) }
        IntField("Offset", offset) { offset = it.coerceAtLeast(0) }
    }
}
