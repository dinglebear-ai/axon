package com.axon.app.ui.management

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.DrawerSubItem

@Composable
fun ManagementDrawerContent(
    onOpenDedupe: () -> Unit,
    onOpenMonitor: () -> Unit,
    onOpenSync: () -> Unit,
    onOpenStack: () -> Unit,
    onOpenSettings: () -> Unit,
) {
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(
            modifier = Modifier
                .fillMaxWidth(0.88f)
                .widthIn(max = 360.dp)
                .padding(vertical = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            DrawerSubItem(
                icon = Icons.Rounded.ContentCopy,
                label = "Dedupe",
                detail = "merge duplicate vectors",
                onClick = onOpenDedupe,
            )
            DrawerSubItem(
                icon = Icons.Rounded.MonitorHeart,
                label = "Monitor",
                detail = "live job + GPU monitor",
                onClick = onOpenMonitor,
            )
            DrawerSubItem(
                icon = Icons.Rounded.Sync,
                label = "Sync",
                detail = "sitemap backfill",
                onClick = onOpenSync,
            )
            DrawerSubItem(
                icon = Icons.Rounded.Storage,
                label = "Stack",
                detail = "compose services",
                onClick = onOpenStack,
            )
            DrawerSubItem(
                icon = Icons.Rounded.Tune,
                label = "Config",
                detail = ".env + config.toml",
                onClick = onOpenSettings,
            )
        }
    }
}
