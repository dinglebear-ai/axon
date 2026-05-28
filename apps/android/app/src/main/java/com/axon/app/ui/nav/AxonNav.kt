package com.axon.app.ui.nav

import androidx.compose.runtime.compositionLocalOf
import kotlinx.serialization.Serializable

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

/** Top-level shell that hosts [RailScaffold] — the navigation rail + content pane. */
@Serializable data object RailShellRoute

/** Full-screen suggest list — accessible from the Knowledge drawer section. */
@Serializable data object SuggestRoute
