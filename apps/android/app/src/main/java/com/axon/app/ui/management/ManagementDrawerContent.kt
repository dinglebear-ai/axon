package com.axon.app.ui.management

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.DrawerSubItem
import com.axon.app.ui.theme.AxonColors

@Composable
fun ManagementDrawerContent(
    onOpenDedupe: () -> Unit,
    onOpenMonitor: () -> Unit,
    onOpenSync: () -> Unit,
    onOpenStack: () -> Unit,
    onOpenSettings: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        DrawerSubItem(
            icon = Icons.Rounded.ContentCopy,
            label = "Dedupe",
            detail = "merge duplicate vectors",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenDedupe,
        )
        DrawerSubItem(
            icon = Icons.Rounded.MonitorHeart,
            label = "Monitor",
            detail = "live job + GPU monitor",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenMonitor,
        )
        DrawerSubItem(
            icon = Icons.Rounded.Sync,
            label = "Sync",
            detail = "sitemap backfill",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenSync,
        )
        DrawerSubItem(
            icon = Icons.Rounded.Storage,
            label = "Stack",
            detail = "compose services",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenStack,
        )
        DrawerSubItem(
            icon = Icons.Rounded.Tune,
            label = "Config",
            detail = ".env + config.toml",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenSettings,
        )
    }
}
