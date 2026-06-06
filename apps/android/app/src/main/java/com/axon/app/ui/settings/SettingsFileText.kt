package com.axon.app.ui.settings

internal fun parseEnvText(raw: String): Map<String, String> =
    raw.lineSequence()
        .map { it.trim() }
        .filter { it.isNotEmpty() && !it.startsWith("#") && "=" in it }
        .associate { line ->
            val key = line.substringBefore("=").trim()
            val value = line.substringAfter("=").trim().unquoteConfigValue()
            key to value
        }

internal fun renderEnvText(values: Map<String, String>): String = buildString {
    AxonSettingsCatalog.envGroups.forEach { group ->
        appendLine("# -- ${group.label} --")
        group.fields.forEach { field ->
            append(field.key)
            append("=")
            appendLine(values[field.key].orEmpty())
        }
        appendLine()
    }
}.trimEnd() + "\n"

internal fun parseConfigTomlText(raw: String): Map<String, String> {
    val out = mutableMapOf<String, String>()
    var section = ""
    raw.lineSequence().forEach { original ->
        val line = original.substringBefore("#").trim()
        if (line.isEmpty()) return@forEach
        if (line.startsWith("[") && line.endsWith("]")) {
            section = line.removePrefix("[").removeSuffix("]").trim()
            return@forEach
        }
        if ("=" !in line || section.isBlank()) return@forEach
        val key = line.substringBefore("=").trim()
        val value = line.substringAfter("=").trim().unquoteConfigValue()
        out["$section.$key"] = value
    }
    return out
}

internal fun renderConfigTomlText(values: Map<String, String>): String = buildString {
    AxonSettingsCatalog.configGroups.forEach { group ->
        val section = group.section?.removePrefix("[")?.removeSuffix("]") ?: group.id
        appendLine("[${section}]")
        group.fields.forEach { field ->
            val key = "${group.id}.${field.key}"
            append(field.key)
            append(" = ")
            appendLine(formatTomlValue(field, values[key].orEmpty()))
        }
        appendLine()
    }
}.trimEnd() + "\n"

private fun String.unquoteConfigValue(): String {
    val trimmed = trim()
    if (trimmed.length >= 2 && trimmed.first() == '"' && trimmed.last() == '"') {
        return trimmed.substring(1, trimmed.length - 1)
    }
    if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
        return trimmed.removePrefix("[").removeSuffix("]")
            .split(",")
            .joinToString(", ") { it.trim().unquoteConfigValue() }
            .trim()
    }
    return trimmed
}

private fun formatTomlValue(field: SettingField, value: String): String =
    when (field.kind) {
        SettingKind.Bool, SettingKind.Int, SettingKind.Float -> value.ifBlank { field.defaultValue }
        SettingKind.List -> {
            val parts = value.split(",").map { it.trim() }.filter { it.isNotEmpty() }
            "[" + parts.joinToString(", ") { "\"${it.escapeToml()}\"" } + "]"
        }
        SettingKind.Text, SettingKind.Secret, SettingKind.Enum -> "\"${value.escapeToml()}\""
    }

private fun String.escapeToml(): String =
    replace("\\", "\\\\").replace("\"", "\\\"")
