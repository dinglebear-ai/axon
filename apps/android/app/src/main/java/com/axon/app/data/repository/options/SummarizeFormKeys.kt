package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.stringPreferencesKey

internal object SummarizeFormKeys {
    val RENDER_MODE      = stringPreferencesKey("mode_options.summarize.render_mode")
    val ROOT_SELECTOR    = stringPreferencesKey("mode_options.summarize.root_selector")
    val EXCLUDE_SELECTOR = stringPreferencesKey("mode_options.summarize.exclude_selector")
    val ALL: List<Preferences.Key<*>> = listOf(RENDER_MODE, ROOT_SELECTOR, EXCLUDE_SELECTOR)
}
