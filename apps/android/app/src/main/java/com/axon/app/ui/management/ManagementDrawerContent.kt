package com.axon.app.ui.management

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.status.ConnectionState
import com.axon.app.ui.status.ConnectionStatusViewModel

private val AccentPrimary = Color(0xFF29B6F6)
private val TextMuted     = Color(0xFFA7BCC9)
private val WarnBase      = Color(0xFFC6A36B)
private val ErrorBase     = Color(0xFFEF5350)
private val SuccessBase   = Color(0xFF66BB6A)
private val TextLabel     = Color(0xFFE1EEF7)

@Composable
fun ManagementDrawerContent(
    onOpenSettings: () -> Unit,
    statusVm: ConnectionStatusViewModel = viewModel(),
    vm: ManagementViewModel = viewModel(),
) {
    val connState by statusVm.state.collectAsStateWithLifecycle()
    val statsState by vm.statsState.collectAsStateWithLifecycle()
    val doctorState by vm.doctorState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        // ── Connection status header ──────────────────────────────────────────
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text("Server", style = MaterialTheme.typography.labelSmall, color = TextMuted)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                val (dotColor, label) = when (connState) {
                    ConnectionState.Checking -> AccentPrimary to "Checking"
                    ConnectionState.Online   -> SuccessBase to "Online"
                    ConnectionState.Offline  -> ErrorBase to "Offline"
                }
                Box(
                    modifier = Modifier
                        .size(7.dp)
                        .let { if (connState != ConnectionState.Offline) it else it },
                ) {
                    androidx.compose.foundation.Canvas(modifier = Modifier.fillMaxSize()) {
                        drawCircle(color = dotColor)
                    }
                }
                Text(label, style = MaterialTheme.typography.labelSmall, color = dotColor)
                Text("·", style = MaterialTheme.typography.labelSmall, color = TextMuted)
                Text(
                    "Refresh",
                    style = MaterialTheme.typography.labelSmall,
                    color = AccentPrimary,
                    modifier = Modifier.clickable(remember { MutableInteractionSource() }, indication = null) {
                        statusVm.refresh()
                    },
                )
            }
        }

        // ── Monitor ───────────────────────────────────────────────────────────
        MgmtSubItem(
            icon = Icons.Rounded.MonitorHeart,
            label = "Monitor",
            detail = when (connState) {
                ConnectionState.Checking -> "Checking…"
                ConnectionState.Online   -> "Server reachable"
                ConnectionState.Offline  -> "Server unreachable"
            },
            detailColor = when (connState) {
                ConnectionState.Checking -> TextMuted
                ConnectionState.Online   -> SuccessBase
                ConnectionState.Offline  -> ErrorBase
            },
        )

        // ── Stack (stats) ─────────────────────────────────────────────────────
        MgmtSubItem(
            icon = Icons.Rounded.Storage,
            label = "Stack",
            detail = when (val s = statsState) {
                is MgmtActionState.Idle    -> "Tap to load"
                is MgmtActionState.Loading -> "Loading…"
                is MgmtActionState.Done    -> s.summary
                is MgmtActionState.Error   -> s.message
            },
            detailColor = when (statsState) {
                is MgmtActionState.Error -> ErrorBase
                else -> TextMuted
            },
            onClick = { vm.loadStats() },
        )

        // ── Dedupe ────────────────────────────────────────────────────────────
        MgmtSubItem(
            icon = Icons.Rounded.ContentCopy,
            label = "Dedupe",
            detail = "Coming soon",
            detailColor = TextMuted,
            badgeLabel = "soon",
            badgeColor = WarnBase,
        )

        // ── Sync ──────────────────────────────────────────────────────────────
        MgmtSubItem(
            icon = Icons.Rounded.Sync,
            label = "Sync",
            detail = "Coming soon",
            detailColor = TextMuted,
            badgeLabel = "soon",
            badgeColor = WarnBase,
        )

        // ── Config ────────────────────────────────────────────────────────────
        MgmtSubItem(
            icon = Icons.Rounded.Tune,
            label = "Config",
            detail = "Server URL, token, collection",
            detailColor = TextMuted,
            onClick = onOpenSettings,
        )
    }
}

@Composable
private fun MgmtSubItem(
    icon: ImageVector,
    label: String,
    detail: String,
    detailColor: Color = TextMuted,
    badgeLabel: String? = null,
    badgeColor: Color = WarnBase,
    onClick: (() -> Unit)? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .let { if (onClick != null) it.clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick) else it }
            .padding(vertical = 8.dp, horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(imageVector = icon, contentDescription = label, tint = if (onClick != null) AccentPrimary else TextMuted, modifier = Modifier.size(17.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.bodySmall, color = TextLabel)
            Text(detail, style = MaterialTheme.typography.labelSmall, color = detailColor)
        }
        if (badgeLabel != null) {
            Text(
                badgeLabel,
                style = MaterialTheme.typography.labelSmall,
                color = badgeColor,
            )
        } else if (onClick != null) {
            Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = TextMuted, modifier = Modifier.size(14.dp))
        }
    }
}
