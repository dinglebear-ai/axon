package com.axon.app.ui.setup

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
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

private val AccentPrimary = Color(0xFF29B6F6)
private val TextMuted     = Color(0xFFA7BCC9)
private val SuccessBase   = Color(0xFF66BB6A)
private val ErrorBase     = Color(0xFFEF5350)
private val TextLabel     = Color(0xFFE1EEF7)

@Composable
fun SetupDrawerContent(
    onOpenSettings: () -> Unit,
    vm: SetupViewModel = viewModel(),
) {
    val smokeState by vm.smokeState.collectAsStateWithLifecycle()
    val doctorState by vm.doctorState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        // ── Preflight (smoke + doctor) ────────────────────────────────────────
        SetupSubItem(
            icon = Icons.Rounded.FlightTakeoff,
            label = "Preflight",
            detail = when {
                smokeState is SetupActionState.Running || doctorState is SetupActionState.Running -> "Running checks…"
                smokeState is SetupActionState.Fail    -> (smokeState as SetupActionState.Fail).message
                doctorState is SetupActionState.Fail   -> (doctorState as SetupActionState.Fail).message
                smokeState is SetupActionState.Pass && doctorState is SetupActionState.Pass -> "All checks passed"
                else -> "Tap to run all checks"
            },
            detailColor = when {
                smokeState is SetupActionState.Fail || doctorState is SetupActionState.Fail -> ErrorBase
                smokeState is SetupActionState.Pass && doctorState is SetupActionState.Pass -> SuccessBase
                else -> TextMuted
            },
            onClick = {
                vm.runSmoke()
                vm.runDoctor()
            },
        )

        // ── Setup (→ Settings) ────────────────────────────────────────────────
        SetupSubItem(
            icon = Icons.Rounded.Construction,
            label = "Setup",
            detail = "Server URL · Token · Collection",
            detailColor = TextMuted,
            onClick = onOpenSettings,
        )

        // ── Smoke ─────────────────────────────────────────────────────────────
        SetupSubItem(
            icon = Icons.Rounded.Wifi,
            label = "Smoke",
            detail = when (val s = smokeState) {
                is SetupActionState.Idle    -> "Tap to run /healthz"
                is SetupActionState.Running -> "Testing connectivity…"
                is SetupActionState.Pass    -> s.detail
                is SetupActionState.Fail    -> s.message
            },
            detailColor = when (smokeState) {
                is SetupActionState.Pass -> SuccessBase
                is SetupActionState.Fail -> ErrorBase
                else -> TextMuted
            },
            onClick = { vm.runSmoke() },
        )

        // ── Doctor ────────────────────────────────────────────────────────────
        SetupSubItem(
            icon = Icons.Rounded.HealthAndSafety,
            label = "Doctor",
            detail = when (val s = doctorState) {
                is SetupActionState.Idle    -> "Tap to run /v1/doctor"
                is SetupActionState.Running -> "Running diagnostics…"
                is SetupActionState.Pass    -> s.detail
                is SetupActionState.Fail    -> s.message
            },
            detailColor = when (doctorState) {
                is SetupActionState.Pass -> SuccessBase
                is SetupActionState.Fail -> ErrorBase
                else -> TextMuted
            },
            onClick = { vm.runDoctor() },
        )

        // ── Debug ─────────────────────────────────────────────────────────────
        SetupSubItem(
            icon = Icons.Rounded.BugReport,
            label = "Debug",
            detail = "Server config · Advanced settings",
            detailColor = TextMuted,
            onClick = onOpenSettings,
        )
    }
}

@Composable
private fun SetupSubItem(
    icon: ImageVector,
    label: String,
    detail: String,
    detailColor: Color = TextMuted,
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
            Text(detail, style = MaterialTheme.typography.labelSmall, color = detailColor, maxLines = 2)
        }
        if (onClick != null) {
            Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = TextMuted, modifier = Modifier.size(14.dp))
        }
    }
}
