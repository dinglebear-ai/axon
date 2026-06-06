package com.axon.app.ui.setup

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.DrawerSubItem
import com.axon.app.ui.theme.AxonColors

@Composable
fun SetupDrawerContent(
    onOpenPreflight: () -> Unit,
    onOpenSetup: () -> Unit,
    onOpenSmoke: () -> Unit,
    onOpenDoctor: () -> Unit,
    onOpenDebug: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        DrawerSubItem(
            icon = Icons.Rounded.FlightTakeoff,
            label = "Preflight",
            detail = "check prerequisites",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenPreflight,
        )
        DrawerSubItem(
            icon = Icons.Rounded.Construction,
            label = "Setup",
            detail = "init + compose up",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenSetup,
        )
        DrawerSubItem(
            icon = Icons.Rounded.Wifi,
            label = "Smoke",
            detail = "crawl/ask proof",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenSmoke,
        )
        DrawerSubItem(
            icon = Icons.Rounded.HealthAndSafety,
            label = "Doctor",
            detail = "service health",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenDoctor,
        )
        DrawerSubItem(
            icon = Icons.Rounded.BugReport,
            label = "Debug",
            detail = "env + paths",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenDebug,
        )
    }
}
