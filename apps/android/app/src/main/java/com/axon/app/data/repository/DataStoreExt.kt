package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.preferencesDataStore

/**
 * Shared DataStore<Preferences> instances used by Phase-2 repositories.
 *
 * Defined as Context extension properties so the singleton-per-process guarantee
 * provided by `preferencesDataStore(...)` is honoured — instantiating
 * [androidx.datastore.preferences.PreferenceDataStoreFactory] more than once for
 * the same file crashes at runtime.
 */
internal val Context.recentJobsDataStore: DataStore<Preferences> by preferencesDataStore("recent_jobs")
internal val Context.modeOptionsDataStore: DataStore<Preferences> by preferencesDataStore("mode_options")
