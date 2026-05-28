package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.axon.app.data.repository.DEFAULT_COLLECTION
import com.axon.app.data.repository.options.IngestFormKeys
import tv.tootie.aurora.components.AuroraTextField


private const val DEFAULT_INCLUDE_SOURCE = true

@Composable
fun IngestOptionsForm() {
    val repo = rememberModeOptionsRepository()
    var includeSource by rememberPersistedState(IngestFormKeys.INCLUDE_SOURCE, DEFAULT_INCLUDE_SOURCE, repo)
    var collection by rememberPersistedState(IngestFormKeys.COLLECTION, DEFAULT_COLLECTION, repo)

    ModeOptionsFormScaffold(
        title = "Ingest options",
        description = "External sources: GitHub / Reddit / YouTube / generic git.",
        resetKeys = IngestFormKeys.ALL,
        repo = repo,
    ) {
        AuroraTextField(
            value = collection,
            onValueChange = { collection = it },
            label = "Collection",
            modifier = Modifier.fillMaxWidth(),
        )
        SwitchRow("Include source code (git providers)", includeSource) { includeSource = it }
    }
}
