package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.axon.app.data.repository.options.SearchWebFormKeys
import tv.tootie.aurora.components.AuroraSelect

private const val DEFAULT_LIMIT = 10
private const val DEFAULT_OFFSET = 0
// Empty string == "unset" — sent as null to the server.
private const val DEFAULT_TIME_RANGE = ""

private val TIME_RANGES = listOf("", "day", "week", "month")

@Composable
fun SearchWebOptionsForm() {
    val repo = rememberModeOptionsRepository()
    var limit by rememberPersistedState(SearchWebFormKeys.LIMIT, DEFAULT_LIMIT, repo)
    var offset by rememberPersistedState(SearchWebFormKeys.OFFSET, DEFAULT_OFFSET, repo)
    var timeRange by rememberPersistedState(SearchWebFormKeys.TIME_RANGE, DEFAULT_TIME_RANGE, repo)

    ModeOptionsFormScaffold(
        title = "Search options",
        description = "Tavily web search; empty time-range == no constraint.",
        resetKeys = SearchWebFormKeys.ALL,
        repo = repo,
    ) {
        IntField("Limit", limit) { limit = it }
        IntField("Offset", offset) { offset = it }
        AuroraSelect(
            selectedOption = if (timeRange.isEmpty()) "(any)" else timeRange,
            onOptionSelected = { picked ->
                timeRange = if (picked == "(any)") "" else picked
            },
            options = TIME_RANGES.map { if (it.isEmpty()) "(any)" else it },
            label = "Time range",
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
