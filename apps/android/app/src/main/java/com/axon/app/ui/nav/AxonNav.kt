package com.axon.app.ui.nav

import androidx.compose.runtime.compositionLocalOf

/**
 * Typed navigation callback for deep children that need to open a document
 * view (e.g. a Query result card). Provided once by [AxonNavGraph] so consumers
 * don't get a handle to the full `NavController`.
 *
 * Throws when read outside a provider so accidental misuse fails loudly.
 */
val LocalOpenDocument = compositionLocalOf<(url: String) -> Unit> {
    error("LocalOpenDocument not provided. Wrap composables under AxonNavGraph.")
}

/**
 * Mode-options trigger surfaced by screens that own an `AuroraPromptInput`.
 * `OperationsScreen` provides a real handler; other call sites get `null` (no cog).
 *
 * Nullable on purpose — screens render the cog only when a handler is present,
 * so reuse outside the Operations host (Document view, etc.) is unaffected.
 */
val LocalModeOptionsCog = compositionLocalOf<(() -> Unit)?> { null }
