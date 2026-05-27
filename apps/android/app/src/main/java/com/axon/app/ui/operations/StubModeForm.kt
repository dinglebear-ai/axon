package com.axon.app.ui.operations

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Build
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.EmptyContent

/**
 * Placeholder body shown for modes that have an API endpoint but no client/UI wiring
 * yet (Summarize, Ingest, real web Search). Kept deliberately bare so the surrounding
 * Operations shell stays focused on navigation; replace per-mode as each is wired.
 */
@Composable
fun StubModeForm(mode: OperationMode, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("${mode.label} mode", style = MaterialTheme.typography.headlineMedium)
        EmptyContent(
            title = "${mode.label} — not yet wired",
            description = "The /v1/${mode.label.lowercase()} endpoint exists server-side. " +
                    "The Android client method and form are coming in a follow-up patch.",
            icon = Icons.Outlined.Build,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
