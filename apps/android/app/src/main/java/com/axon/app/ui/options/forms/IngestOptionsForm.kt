package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import com.axon.app.data.repository.options.IngestFormKeys

private const val DEFAULT_INCLUDE_SOURCE = true

@Composable
fun IngestOptionsForm() {
    val repo = rememberModeOptionsRepository()
    var includeSource by rememberPersistedState(IngestFormKeys.INCLUDE_SOURCE, DEFAULT_INCLUDE_SOURCE, repo)

    ModeOptionsFormScaffold(
        title = "Ingest options",
        description = "External sources: GitHub / Reddit / YouTube / generic git.",
        resetKeys = IngestFormKeys.ALL,
        repo = repo,
    ) {
        SwitchRow("Include source code (git providers)", includeSource) { includeSource = it }
    }
}
