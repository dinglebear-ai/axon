package com.axon.app.ui.options.forms

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import com.axon.app.data.repository.options.ResearchFormKeys

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
