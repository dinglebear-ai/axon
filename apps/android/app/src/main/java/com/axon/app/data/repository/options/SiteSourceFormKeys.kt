package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey

internal object SiteSourceFormKeys {
    val MAX_PAGES = intPreferencesKey("mode_options.source_site.max_pages")
    val MAX_DEPTH = intPreferencesKey("mode_options.source_site.max_depth")
    val RENDER_MODE = stringPreferencesKey("mode_options.source_site.render_mode")
    val INCLUDE_SUBDOMAINS = booleanPreferencesKey("mode_options.source_site.include_subdomains")
    val SKIP_EMBED = booleanPreferencesKey("mode_options.source_site.skip_embed")
    val COLLECTION = stringPreferencesKey("mode_options.source_site.collection")

    val ALL: List<Preferences.Key<*>> =
        listOf(
            MAX_PAGES,
            MAX_DEPTH,
            RENDER_MODE,
            INCLUDE_SUBDOMAINS,
            SKIP_EMBED,
            COLLECTION,
        )
}
