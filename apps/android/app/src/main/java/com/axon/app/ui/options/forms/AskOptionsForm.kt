package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import tv.tootie.aurora.components.AuroraSwitch
import tv.tootie.aurora.components.AuroraTextField

/** Internal — keys exposed to [com.axon.app.data.repository.ModeOptionsRepository.apply] only. */
internal object AskFormKeys {
    val CHUNK_LIMIT       = intPreferencesKey("mode_options.ask.chunk_limit")
    val FULL_DOCS         = intPreferencesKey("mode_options.ask.full_docs")
    val MAX_CONTEXT_CHARS = intPreferencesKey("mode_options.ask.max_context_chars")
    val HYBRID_CANDIDATES = intPreferencesKey("mode_options.ask.hybrid_candidates")
    val DIAGNOSTICS       = booleanPreferencesKey("mode_options.ask.diagnostics")
    val EXPLAIN           = booleanPreferencesKey("mode_options.ask.explain")
    val COLLECTION        = stringPreferencesKey("mode_options.ask.collection")

    val ALL: List<Preferences.Key<*>> =
        listOf(CHUNK_LIMIT, FULL_DOCS, MAX_CONTEXT_CHARS, HYBRID_CANDIDATES, DIAGNOSTICS, EXPLAIN, COLLECTION)
}

private const val DEFAULT_CHUNK_LIMIT = 20
private const val DEFAULT_FULL_DOCS = 6
private const val DEFAULT_MAX_CONTEXT_CHARS = 300_000
private const val DEFAULT_HYBRID_CANDIDATES = 100
private const val DEFAULT_DIAGNOSTICS = false
private const val DEFAULT_EXPLAIN = false
private const val DEFAULT_COLLECTION = "axon"

@Composable
fun AskOptionsForm() {
    val repo = rememberModeOptionsRepository()
    var chunkLimit by rememberPersistedState(AskFormKeys.CHUNK_LIMIT, DEFAULT_CHUNK_LIMIT, repo)
    var fullDocs by rememberPersistedState(AskFormKeys.FULL_DOCS, DEFAULT_FULL_DOCS, repo)
    var maxCtx by rememberPersistedState(AskFormKeys.MAX_CONTEXT_CHARS, DEFAULT_MAX_CONTEXT_CHARS, repo)
    var hybridCandidates by rememberPersistedState(AskFormKeys.HYBRID_CANDIDATES, DEFAULT_HYBRID_CANDIDATES, repo)
    var diagnostics by rememberPersistedState(AskFormKeys.DIAGNOSTICS, DEFAULT_DIAGNOSTICS, repo)
    var explain by rememberPersistedState(AskFormKeys.EXPLAIN, DEFAULT_EXPLAIN, repo)
    var collection by rememberPersistedState(AskFormKeys.COLLECTION, DEFAULT_COLLECTION, repo)

    ModeOptionsFormScaffold(
        title = "Ask options",
        description = "Knobs forwarded to /v1/ask. Empty values fall through to server defaults.",
        resetKeys = AskFormKeys.ALL,
        repo = repo,
    ) {
        IntField("Chunk limit", chunkLimit) { chunkLimit = it }
        IntField("Full docs", fullDocs) { fullDocs = it }
        IntField("Max context chars", maxCtx) { maxCtx = it }
        IntField("Hybrid candidates", hybridCandidates) { hybridCandidates = it }
        AuroraTextField(
            value = collection,
            onValueChange = { collection = it },
            label = "Collection",
            modifier = Modifier.fillMaxWidth(),
        )
        SwitchRow("Diagnostics", diagnostics) { diagnostics = it }
        SwitchRow("Explain", explain) { explain = it }
    }
}

@Composable
internal fun IntField(label: String, value: Int, onValueChange: (Int) -> Unit) {
    AuroraTextField(
        value = value.toString(),
        onValueChange = { raw ->
            raw.filter { it.isDigit() }.toIntOrNull()?.let(onValueChange)
        },
        label = label,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
internal fun SwitchRow(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label)
        AuroraSwitch(
            checked = checked,
            onCheckedChange = onCheckedChange,
            contentDescription = label,
            modifier = Modifier.padding(start = 12.dp),
        )
    }
}
