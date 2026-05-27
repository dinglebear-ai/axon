package com.axon.app.ui.operations

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Build
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.NotYetWiredPage

@Composable
fun StubModeForm(mode: OperationMode, modifier: Modifier = Modifier) {
    NotYetWiredPage(
        title = "${mode.label} mode",
        headline = "${mode.label} — not yet wired",
        description = "Not yet implemented in the Android client. " +
                "The ${mode.endpointPath} endpoint is reachable from the server.",
        icon = Icons.Outlined.Build,
        modifier = modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
    )
}
