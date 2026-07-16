package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.axon.app.data.repository.options.SourceFormKeys
import tv.tootie.aurora.components.AuroraTextField

private const val DEFAULT_EMBED = true

@Composable
fun SourceOptionsForm() {
    val repo = rememberModeOptionsRepository()
    var embed by rememberPersistedState(SourceFormKeys.EMBED, DEFAULT_EMBED, repo)
    var collection by rememberPersistedState(SourceFormKeys.COLLECTION, "", repo)

    ModeOptionsFormScaffold(
        title = "Source options",
        description = "External sources: GitHub / Reddit / YouTube / generic git.",
        resetKeys = SourceFormKeys.ALL,
        repo = repo,
    ) {
        SwitchRow("Embed and publish", embed) { embed = it }
        AuroraTextField(
            value = collection,
            onValueChange = { collection = it },
            label = "Collection override",
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
