package com.axon.app.ui.setup

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.DrawerSubItem

@Composable
fun SetupDrawerContent(
    onOpenPreflight: () -> Unit,
    onOpenSetup: () -> Unit,
    onOpenSmoke: () -> Unit,
    onOpenDoctor: () -> Unit,
    onOpenDebug: () -> Unit,
) {
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(
            modifier = Modifier
                .fillMaxWidth(0.88f)
                .widthIn(max = 360.dp)
                .padding(top = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            DrawerSubItem(
                icon = Icons.Rounded.FlightTakeoff,
                label = "Preflight",
                detail = "check prerequisites",
                onClick = onOpenPreflight,
            )
            DrawerSubItem(
                icon = Icons.Rounded.Construction,
                label = "Setup",
                detail = "init + compose up",
                onClick = onOpenSetup,
            )
            DrawerSubItem(
                icon = Icons.Rounded.Wifi,
                label = "Smoke",
                detail = "crawl/ask proof",
                onClick = onOpenSmoke,
            )
            DrawerSubItem(
                icon = Icons.Rounded.HealthAndSafety,
                label = "Doctor",
                detail = "service health",
                onClick = onOpenDoctor,
            )
            DrawerSubItem(
                icon = Icons.Rounded.BugReport,
                label = "Debug",
                detail = "env + paths",
                onClick = onOpenDebug,
            )
        }
    }
}
