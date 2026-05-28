package com.axon.app.ui.setup

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.DrawerSubItem
import com.axon.app.ui.theme.AxonColors

// ViewModel is activity-scoped (default ViewModelStoreOwner), so smoke/doctor
// results survive across drawer open/close cycles — intentional.
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
        DrawerSubItem(
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
                smokeState is SetupActionState.Fail || doctorState is SetupActionState.Fail -> AxonColors.ErrorBase
                smokeState is SetupActionState.Pass && doctorState is SetupActionState.Pass -> AxonColors.SuccessBase
                else -> AxonColors.TextMuted
            },
            onClick = {
                vm.runSmoke()
                vm.runDoctor()
            },
            trailing = { Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = AxonColors.TextMuted, modifier = Modifier.size(14.dp)) },
        )

        // ── Setup (→ Settings) ────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.Construction,
            label = "Setup",
            detail = "Server URL · Token · Collection",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenSettings,
            trailing = { Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = AxonColors.TextMuted, modifier = Modifier.size(14.dp)) },
        )

        // ── Smoke ─────────────────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.Wifi,
            label = "Smoke",
            detail = when (val s = smokeState) {
                is SetupActionState.Idle    -> "Tap to run /healthz"
                is SetupActionState.Running -> "Testing connectivity…"
                is SetupActionState.Pass    -> s.detail
                is SetupActionState.Fail    -> s.message
            },
            detailColor = when (smokeState) {
                is SetupActionState.Pass -> AxonColors.SuccessBase
                is SetupActionState.Fail -> AxonColors.ErrorBase
                else -> AxonColors.TextMuted
            },
            onClick = { vm.runSmoke() },
            trailing = { Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = AxonColors.TextMuted, modifier = Modifier.size(14.dp)) },
        )

        // ── Doctor ────────────────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.HealthAndSafety,
            label = "Doctor",
            detail = when (val s = doctorState) {
                is SetupActionState.Idle    -> "Tap to run /v1/doctor"
                is SetupActionState.Running -> "Running diagnostics…"
                is SetupActionState.Pass    -> s.detail
                is SetupActionState.Fail    -> s.message
            },
            detailColor = when (doctorState) {
                is SetupActionState.Pass -> AxonColors.SuccessBase
                is SetupActionState.Fail -> AxonColors.ErrorBase
                else -> AxonColors.TextMuted
            },
            onClick = { vm.runDoctor() },
            trailing = { Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = AxonColors.TextMuted, modifier = Modifier.size(14.dp)) },
        )

        // ── Debug ─────────────────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.BugReport,
            label = "Debug",
            detail = "Server config · Advanced settings",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenSettings,
            trailing = { Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = AxonColors.TextMuted, modifier = Modifier.size(14.dp)) },
        )
    }
}
