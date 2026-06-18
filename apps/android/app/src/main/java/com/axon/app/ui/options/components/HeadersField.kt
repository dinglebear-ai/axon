package com.axon.app.ui.options.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.AxonSensitiveTextField
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
    // Strip CR/LF/NUL to prevent header injection attacks.
    val k = key.trim().replace(Regex("[\r\n\\u0000]"), "")
    if (k.isEmpty()) return null
    val v = value.replace(Regex("[\r\n\\u0000]"), "")
    return "$k: $v"
}

/**
 * Parse a `"Key: Value"` wire line into its (key, value) pair. Whitespace around
 * the key is trimmed; whitespace after the first `:` is trimmed once.
 *
 * - `"Foo: bar"`     → `("Foo", "bar")`
 * - `"foo:bar:baz"` → `("foo", "bar:baz")` — only the first colon splits
 * - `"orphan"`       → `("", "orphan")` — header missing a key, preserved so
 *                      the user can fix it without losing typed text
 * - empty / `:value` → `("", value.trim())`
 */
internal fun splitHeader(line: String): Pair<String, String> {
    val idx = line.indexOf(':')
    if (idx < 0) return "" to line.trim()
    // idx == 0 means a leading colon — the key is empty but we still want to
    // strip the colon from the value so a `":value"` round-trip becomes
    // `("", "value")` (matching the wire-format split contract).
    return line.substring(0, idx).trim() to line.substring(idx + 1).trim()
}

/**
 * Pure state reducer for [HeadersField]. Exposed for unit testing — the
 * composable wires this into a `SnapshotStateList<Pair<String, String>>`.
 *
 * Operations are immutable: each call returns a fresh list so the caller can
 * publish it without worrying about list-identity tricks. Empty input becomes
 * a single empty row so the user always sees one slot to type into.
 */
internal object HeadersReducer {

    /** Build the initial row list from a List of wire `"Key: Value"` strings. */
    fun init(wire: List<String>): List<Pair<String, String>> {
        val parsed = wire.map(::splitHeader)
        return if (parsed.isEmpty()) listOf("" to "") else parsed
    }

    fun setKey(rows: List<Pair<String, String>>, index: Int, key: String): List<Pair<String, String>> =
        if (index !in rows.indices) rows else rows.toMutableList().also { it[index] = key to it[index].second }

    fun setValue(rows: List<Pair<String, String>>, index: Int, value: String): List<Pair<String, String>> =
        if (index !in rows.indices) rows else rows.toMutableList().also { it[index] = it[index].first to value }

    fun addBlank(rows: List<Pair<String, String>>): List<Pair<String, String>> =
        rows + ("" to "")

    fun remove(rows: List<Pair<String, String>>, index: Int): List<Pair<String, String>> {
        if (index !in rows.indices) return rows
        val next = rows.toMutableList().also { it.removeAt(index) }
        return if (next.isEmpty()) listOf("" to "") else next
    }

    /** Serialize the row list to the wire format, skipping rows with a blank key. */
    fun toWire(rows: List<Pair<String, String>>): List<String> =
        rows.mapNotNull { (k, v) -> joinHeader(k, v) }
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
    // Seed local state from `headers` ONCE on first composition. Re-keying on
    // `headers` (the old behaviour) discarded in-progress keystrokes whenever
    // the parent's persistence layer round-tripped a new list back through us.
    // The persistence side mirrors what we emit, so the seed value is the
    // canonical input — subsequent edits flow through this state.
    var rows by remember { mutableStateOf(HeadersReducer.init(headers)) }

    fun publish(next: List<Pair<String, String>>) {
        rows = next
        onChange(HeadersReducer.toWire(next))
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
                onKeyChange = { newKey -> publish(HeadersReducer.setKey(rows, index, newKey)) },
                onValueChange = { newValue -> publish(HeadersReducer.setValue(rows, index, newValue)) },
                onDelete = {
                    publish(HeadersReducer.remove(rows, index))
                },
            )
        }
        AuroraButton(
            onClick = { publish(HeadersReducer.addBlank(rows)) },
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
    onDelete: () -> Unit,
) {
    val sensitive = isSensitiveHeaderKey(key)

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
        if (sensitive) {
            AxonSensitiveTextField(
                value = value,
                onValueChange = onValueChange,
                label = "Value",
                revealContentDescription = "Show header value",
                hideContentDescription = "Hide header value",
                modifier = Modifier.weight(0.6f),
            )
        } else {
            AuroraTextField(
                value = value,
                onValueChange = onValueChange,
                label = "Value",
                modifier = Modifier.weight(0.6f),
            )
        }
        IconButton(onClick = onDelete) {
            Icon(Icons.Outlined.Delete, contentDescription = "Remove header")
        }
    }
}
