package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import tv.tootie.aurora.components.AuroraTextField

internal object IngestFormKeys {
    val INCLUDE_SOURCE = booleanPreferencesKey("mode_options.ingest.include_source")
    val COLLECTION     = stringPreferencesKey("mode_options.ingest.collection")
    val ALL: List<Preferences.Key<*>> = listOf(INCLUDE_SOURCE, COLLECTION)
}

private const val DEFAULT_INCLUDE_SOURCE = true
private const val DEFAULT_COLLECTION = "axon"

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
