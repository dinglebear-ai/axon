package com.axon.app.ui.operations

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.ask.AskViewModel
import com.axon.app.ui.nav.LocalModeOptionsCog
import com.axon.app.ui.nav.LocalOpenModeOptions

/**
 * The Operations page. The body is the active mode's screen; the FAB is a
 * draggable, floating button that triggers the mode picker. Each underlying
 * screen renders its own `AuroraPromptInput` — they read [LocalModeOptionsCog]
 * to surface an inline cog left of the Send button.
 */
@Composable
fun OperationsScreen(vm: OperationsViewModel = viewModel()) {
    val activeMode by vm.activeMode.collectAsStateWithLifecycle()
    var sheetVisible by remember { mutableStateOf(false) }

    // R14: rememberSaveable so the prior-mode tracker survives rotation —
    // OperationMode is an enum, supported natively by the default Saver.
    val askVm: AskViewModel = viewModel()
    var previousMode by rememberSaveable { mutableStateOf<OperationMode?>(null) }
    LaunchedEffect(activeMode) {
        if (previousMode == OperationMode.Ask && activeMode != OperationMode.Ask) {
            askVm.clearFollowUp()
        }
        previousMode = activeMode
    }

    // Mode-options cog handler — provided to every prompt input via CompositionLocal.
    // Navigates to the ModeOptionsScreen for the currently active mode.
    val openModeOptions = LocalOpenModeOptions.current
    val onModeOptions = remember(activeMode, openModeOptions) {
        { openModeOptions(activeMode) }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        CompositionLocalProvider(LocalModeOptionsCog provides onModeOptions) {
            ModeContentHost(activeMode = activeMode)
        }

        DraggableFab(
            onClick = { sheetVisible = true },
            content = { Icon(activeMode.icon, contentDescription = activeMode.label) },
        )
    }

    if (sheetVisible) {
        ModePickerSheet(
            activeMode = activeMode,
            onSelect = { mode ->
                vm.setMode(mode)
                sheetVisible = false
            },
            onDismiss = { sheetVisible = false },
        )
    }
}
