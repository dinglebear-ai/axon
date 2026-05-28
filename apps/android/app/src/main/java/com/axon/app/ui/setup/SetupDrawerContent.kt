package com.axon.app.ui.setup

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.DrawerSubItem
import com.axon.app.ui.common.Resource
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
        // Failure surfaces before in-progress so a fast-failing smoke check is visible
        // even while doctor is still running.
        val smokeFail = smokeState as? Resource.Error
        val doctorFail = doctorState as? Resource.Error
        DrawerSubItem(
            icon = Icons.Rounded.FlightTakeoff,
            label = "Preflight",
            detail = when {
                smokeFail != null  -> smokeFail.message
                doctorFail != null -> doctorFail.message
                smokeState is Resource.Loading || doctorState is Resource.Loading -> "Running checks…"
                smokeState is Resource.Ready && doctorState is Resource.Ready -> "All checks passed"
                else -> "Tap to run all checks"
            },
            detailColor = when {
                smokeFail != null || doctorFail != null -> AxonColors.ErrorBase
                smokeState is Resource.Ready && doctorState is Resource.Ready -> AxonColors.SuccessBase
                else -> AxonColors.TextMuted
            },
            onClick = {
                vm.runSmoke()
                vm.runDoctor()
            },
        )

        // ── Setup (→ Settings) ────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.Construction,
            label = "Setup",
            detail = "Server URL · Token · Collection",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenSettings,
        )

        // ── Smoke ─────────────────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.Wifi,
            label = "Smoke",
            detail = when (val s = smokeState) {
                Resource.Idle    -> "Tap to run /healthz"
                Resource.Loading -> "Testing connectivity…"
                is Resource.Ready -> s.value
                is Resource.Error -> s.message
            },
            detailColor = when (smokeState) {
                is Resource.Ready -> AxonColors.SuccessBase
                is Resource.Error -> AxonColors.ErrorBase
                else -> AxonColors.TextMuted
            },
            onClick = { vm.runSmoke() },
        )

        // ── Doctor ────────────────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.HealthAndSafety,
            label = "Doctor",
            detail = when (val s = doctorState) {
                Resource.Idle    -> "Tap to run /v1/doctor"
                Resource.Loading -> "Running diagnostics…"
                is Resource.Ready -> s.value
                is Resource.Error -> s.message
            },
            detailColor = when (doctorState) {
                is Resource.Ready -> AxonColors.SuccessBase
                is Resource.Error -> AxonColors.ErrorBase
                else -> AxonColors.TextMuted
            },
            onClick = { vm.runDoctor() },
        )

        // ── Debug ─────────────────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.BugReport,
            label = "Debug",
            detail = "Server config · Advanced settings",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenSettings,
        )
    }
}
