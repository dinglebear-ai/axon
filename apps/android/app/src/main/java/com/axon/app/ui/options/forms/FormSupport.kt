package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.datastore.preferences.core.Preferences
import com.axon.app.AxonApp
import com.axon.app.data.repository.ModeOptionsRepository
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant

/**
 * Shared scaffolding for every per-mode options form. Each form passes its
 * own list of [Preferences.Key]s used by the "Reset to defaults" button so
 * the repository can wipe only that mode's keys.
 *
 * Forms render their controls into [content]; the scaffold handles the
 * scrolling column, the divider, and the reset button placement.
 */
@Composable
internal fun ModeOptionsFormScaffold(
    title: String,
    description: String?,
    resetKeys: List<Preferences.Key<*>>,
    repo: ModeOptionsRepository,
    /** Optional hook for clearing storage outside of DataStore (e.g. encrypted headers). */
    onResetExtra: suspend () -> Unit = {},
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(title, style = MaterialTheme.typography.titleMedium)
        if (description != null) {
            Text(
                description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        content()
        Row(
            modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
            horizontalArrangement = Arrangement.End,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AuroraButton(
                onClick = {
                    scope.launch {
                        repo.resetKeys(resetKeys)
                        onResetExtra()
                    }
                },
                variant = AuroraButtonVariant.Outlined,
            ) { Text("Reset to defaults") }
        }
    }
}

/** Acquires the singleton [ModeOptionsRepository] from the application container. */
@Composable
internal fun rememberModeOptionsRepository(): ModeOptionsRepository {
    val ctx = LocalContext.current
    return remember(ctx) { (ctx.applicationContext as AxonApp).container.modeOptionsRepository }
}

/**
 * Bound mutable state backed by DataStore. Reads the persisted value on first
 * composition; writes back on every mutation. Use for one preference key.
 */
@Composable
internal fun <T : Any> rememberPersistedState(
    key: Preferences.Key<T>,
    default: T,
    repo: ModeOptionsRepository,
): androidx.compose.runtime.MutableState<T> {
    val scope = rememberCoroutineScope()
    var state by remember(key) { mutableStateOf(default) }
    LaunchedEffect(key) {
        runCatching {
            val stored = repo.read(key)
            if (stored != null) state = stored
        }
    }
    return object : androidx.compose.runtime.MutableState<T> {
        override var value: T
            get() = state
            set(newValue) {
                state = newValue
                scope.launch { repo.write(key, newValue) }
            }
        override fun component1(): T = value
        override fun component2(): (T) -> Unit = { value = it }
    }
}

/** Same as [rememberPersistedState] but holds nullable T (a `null` clears the key). */
@Composable
internal fun <T : Any> rememberOptionalPersistedState(
    key: Preferences.Key<T>,
    repo: ModeOptionsRepository,
): androidx.compose.runtime.MutableState<T?> {
    val scope = rememberCoroutineScope()
    var state by remember(key) { mutableStateOf<T?>(null) }
    LaunchedEffect(key) {
        runCatching { state = repo.read(key) }
    }
    return object : androidx.compose.runtime.MutableState<T?> {
        override var value: T?
            get() = state
            set(newValue) {
                state = newValue
                scope.launch { repo.write(key, newValue) }
            }
        override fun component1(): T? = value
        override fun component2(): (T?) -> Unit = { value = it }
    }
}

