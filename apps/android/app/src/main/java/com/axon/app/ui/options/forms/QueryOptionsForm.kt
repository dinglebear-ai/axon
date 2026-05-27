package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import tv.tootie.aurora.components.AuroraTextField

internal object QueryFormKeys {
    val LIMIT      = intPreferencesKey("mode_options.query.limit")
    val COLLECTION = stringPreferencesKey("mode_options.query.collection")
    val ALL: List<Preferences.Key<*>> = listOf(LIMIT, COLLECTION)
}

private const val DEFAULT_LIMIT = 10
private const val DEFAULT_COLLECTION = "axon"

@Composable
fun QueryOptionsForm() {
    val repo = rememberModeOptionsRepository()
    var limit by rememberPersistedState(QueryFormKeys.LIMIT, DEFAULT_LIMIT, repo)
    var collection by rememberPersistedState(QueryFormKeys.COLLECTION, DEFAULT_COLLECTION, repo)

    ModeOptionsFormScaffold(
        title = "Query options",
        description = "Semantic vector search over indexed content.",
        resetKeys = QueryFormKeys.ALL,
        repo = repo,
    ) {
        IntField("Limit", limit) { limit = it }
        AuroraTextField(
            value = collection,
            onValueChange = { collection = it },
            label = "Collection",
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
