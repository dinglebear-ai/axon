package com.axon.app.ui.options.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Visibility
import androidx.compose.material.icons.outlined.VisibilityOff
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraTextField

/**
 * Sensitive header keys whose values must be visually redacted by default.
 *
 * Pure function exposed for unit testing — the form has no business deciding
 * which keys are sensitive; this list is the single source of truth.
 */
internal val SENSITIVE_HEADER_KEYS: Set<String> = setOf(
    "authorization",
    "cookie",
    "x-api-key",
    "proxy-authorization",
    "x-auth-token",
)

internal fun isSensitiveHeaderKey(key: String): Boolean =
    key.trim().lowercase() in SENSITIVE_HEADER_KEYS

/**
 * Parse a header row's text representations into a single "Key: Value" string
 * suitable for the wire `headers: Vec<String>` field. Returns null when key
 * is blank — caller should skip the row.
 */
internal fun joinHeader(key: String, value: String): String? {
    val k = key.trim()
    if (k.isEmpty()) return null
    return "$k: $value"
}

/**
 * Repeatable Key:Value header rows. Sensitive keys (Authorization, Cookie,
 * X-Api-Key, Proxy-Authorization, X-Auth-Token) mask their value field with
 * [PasswordVisualTransformation] until the user taps the eye toggle.
 *
 * State is hoisted via [headers] / [onChange] — both sides hold "Key: Value"
 * strings (the wire format). Empty rows are filtered out before persistence.
 */
@Composable
fun HeadersField(
    headers: List<String>,
    onChange: (List<String>) -> Unit,
    modifier: Modifier = Modifier,
) {
    // Local mutable list of (key, value) pairs derived from the input.
    val rows = remember(headers) {
        val parsed = headers.map { line ->
            val idx = line.indexOf(':')
            if (idx <= 0) "" to line
            else line.substring(0, idx).trim() to line.substring(idx + 1).trim()
        }
        // Always show at least one empty row so the user can add the first header.
        if (parsed.isEmpty()) mutableStateListOfPairs(listOf("" to ""))
        else mutableStateListOfPairs(parsed)
    }

    // Per-row "show plaintext" toggle map. Keyed by row index because rows can
    // be reordered/removed; remember to drop stale keys when rows shrink.
    val revealed = remember { mutableStateMapOf<Int, Boolean>() }

    fun emit() {
        onChange(rows.mapNotNull { (k, v) -> joinHeader(k, v) })
    }

    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("Headers", style = MaterialTheme.typography.labelMedium)
        rows.forEachIndexed { index, (key, value) ->
            HeaderRow(
                key = key,
                value = value,
                onKeyChange = { newKey ->
                    rows[index] = newKey to value
                    emit()
                },
                onValueChange = { newValue ->
                    rows[index] = key to newValue
                    emit()
                },
                revealed = revealed[index] == true,
                onToggleReveal = { revealed[index] = !(revealed[index] ?: false) },
                onDelete = {
                    rows.removeAt(index)
                    if (rows.isEmpty()) rows.add("" to "")
                    revealed.clear()
                    emit()
                },
            )
        }
        AuroraButton(
            onClick = {
                rows.add("" to "")
                emit()
            },
            variant = AuroraButtonVariant.Outlined,
        ) { Text("Add header") }
    }
}

@Composable
private fun HeaderRow(
    key: String,
    value: String,
    onKeyChange: (String) -> Unit,
    onValueChange: (String) -> Unit,
    revealed: Boolean,
    onToggleReveal: () -> Unit,
    onDelete: () -> Unit,
) {
    val sensitive = isSensitiveHeaderKey(key)
    val transform: VisualTransformation =
        if (sensitive && !revealed) PasswordVisualTransformation() else VisualTransformation.None

    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        AuroraTextField(
            value = key,
            onValueChange = onKeyChange,
            label = "Key",
            modifier = Modifier.weight(0.4f),
        )
        AuroraTextField(
            value = value,
            onValueChange = onValueChange,
            label = "Value",
            visualTransformation = transform,
            modifier = Modifier.weight(0.6f),
            trailingIcon = if (sensitive) {
                {
                    IconButton(onClick = onToggleReveal) {
                        Icon(
                            imageVector = if (revealed) Icons.Outlined.VisibilityOff else Icons.Outlined.Visibility,
                            contentDescription = if (revealed) "Hide value" else "Show value",
                        )
                    }
                }
            } else null,
        )
        IconButton(onClick = onDelete) {
            Icon(Icons.Outlined.Delete, contentDescription = "Remove header")
        }
    }
}

// SnapshotStateList-of-pairs helper. Compose tracks list mutation on the outer
// SnapshotStateList; the inner Pairs are immutable so reassigning rows[i]
// triggers recomposition correctly.
private fun mutableStateListOfPairs(initial: List<Pair<String, String>>) =
    androidx.compose.runtime.mutableStateListOf<Pair<String, String>>().apply { addAll(initial) }

// Small helper to keep a per-row toggled flag without requiring composed state hoisting.
@Suppress("unused")
private fun rememberRevealState() = mutableStateOf(false)
