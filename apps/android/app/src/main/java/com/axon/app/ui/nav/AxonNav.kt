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
