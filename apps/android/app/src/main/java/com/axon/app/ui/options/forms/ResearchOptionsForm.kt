package com.axon.app.ui.options.forms

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.intPreferencesKey

internal object ResearchFormKeys {
    val LIMIT = intPreferencesKey("mode_options.research.limit")
    val ALL: List<Preferences.Key<*>> = listOf(LIMIT)
}

private const val DEFAULT_LIMIT = 10

@Composable
fun ResearchOptionsForm() {
    val repo = rememberModeOptionsRepository()
    var limit by rememberPersistedState(ResearchFormKeys.LIMIT, DEFAULT_LIMIT, repo)

    ModeOptionsFormScaffold(
        title = "Research options",
        description = "Web research via Tavily + LLM synthesis.",
        resetKeys = ResearchFormKeys.ALL,
        repo = repo,
    ) {
        IntField("Limit", limit) { limit = it }
    }
}
