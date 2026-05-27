package com.axon.app.ui.nav

import androidx.compose.runtime.compositionLocalOf
import androidx.navigation.NavController

/**
 * Hoisted [NavController] for deep children that need to navigate (e.g. the Query
 * result card opening a Document view). Provided once in [AxonNavGraph] so we
 * don't prop-drill the controller through `OperationsScreen` → mode → screen.
 *
 * Throws when read outside a provider so accidental misuse is surfaced loudly
 * rather than producing silent no-op navigation.
 */
val LocalAxonNavController = compositionLocalOf<NavController> {
    error("LocalAxonNavController not provided. Wrap composables under AxonNavGraph.")
}
