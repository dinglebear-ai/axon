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

internal fun patchEnvText(raw: String, values: Map<String, String>, dirtyKeys: Set<String>): String {
    if (dirtyKeys.isEmpty()) return raw

    val seen = mutableSetOf<String>()
    val patched = raw.lineSequence().map { line ->
        val trimmed = line.trimStart()
        if (trimmed.isEmpty() || trimmed.startsWith("#") || "=" !in line) {
            line
        } else {
            val key = line.substringBefore("=").trim()
            if (key in dirtyKeys) {
                seen += key
                "$key=${formatEnvValue(values[key].orEmpty())}"
            } else {
                line
            }
        }
    }.toMutableList()

    dirtyKeys.sorted().filterNot { it in seen }.forEach { key ->
        patched += "$key=${formatEnvValue(values[key].orEmpty())}"
    }

    return patched.joinToString("\n").trimEnd() + "\n"
}

internal fun redactEnvText(raw: String, secretKeys: Set<String>): String =
    raw.lineSequence().map { line ->
        val trimmed = line.trimStart()
        if (trimmed.isEmpty() || trimmed.startsWith("#") || "=" !in line) {
            line
        } else {
            val key = line.substringBefore("=").trim()
            if (key in secretKeys && line.substringAfter("=").trim().isNotBlank()) {
                "$key=${formatEnvValue(REDACTED_SECRET_VALUE)}"
            } else {
                line
            }
        }
    }.joinToString("\n")

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

internal fun patchConfigTomlText(raw: String, values: Map<String, String>, dirtyKeys: Set<String>): String {
    if (dirtyKeys.isEmpty()) return raw

    val fieldsByConfigKey = AxonSettingsCatalog.configGroups
        .flatMap { group -> group.fields.map { field -> "${group.id}.${field.key}" to field } }
        .toMap()
    val seen = mutableSetOf<String>()
    var section = ""
    val patched = raw.lineSequence().map { line ->
        val trimmed = line.trim()
        if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
            section = trimmed.removePrefix("[").removeSuffix("]").trim()
            line
        } else if (section.isNotBlank() && "=" in trimmed && !trimmed.startsWith("#")) {
            val key = trimmed.substringBefore("=").trim()
            val configKey = "$section.$key"
            val field = fieldsByConfigKey[configKey]
            if (configKey in dirtyKeys && field != null) {
                seen += configKey
                "$key = ${formatTomlValue(field, values[configKey].orEmpty())}"
            } else {
                line
            }
        } else {
            line
        }
    }.toMutableList()

    val missingBySection = dirtyKeys
        .filterNot { it in seen }
        .mapNotNull { configKey ->
            val field = fieldsByConfigKey[configKey] ?: return@mapNotNull null
            val sectionName = configKey.substringBeforeLast(".")
            sectionName to (field to values[configKey].orEmpty())
        }
        .groupBy({ it.first }, { it.second })

    missingBySection.toSortedMap().forEach { (sectionName, fields) ->
        if (patched.isNotEmpty() && patched.last().isNotBlank()) patched += ""
        patched += "[$sectionName]"
        fields.sortedBy { it.first.key }.forEach { (field, value) ->
            patched += "${field.key} = ${formatTomlValue(field, value)}"
        }
    }

    return patched.joinToString("\n").trimEnd() + "\n"
}

internal fun redactConfigTomlText(raw: String, secretKeys: Set<String>): String {
    var section = ""
    return raw.lineSequence().map { line ->
        val trimmed = line.trim()
        if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
            section = trimmed.removePrefix("[").removeSuffix("]").trim()
            line
        } else if (section.isNotBlank() && "=" in trimmed && !trimmed.startsWith("#")) {
            val key = trimmed.substringBefore("=").trim()
            val configKey = "$section.$key"
            if (configKey in secretKeys && trimmed.substringAfter("=").trim().isNotBlank()) {
                "$key = \"${REDACTED_SECRET_VALUE.escapeToml()}\""
            } else {
                line
            }
        } else {
            line
        }
    }.joinToString("\n")
}

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

private fun formatEnvValue(value: String): String {
    require(!value.contains('\n') && !value.contains('\r')) {
        ".env values cannot contain newlines"
    }
    if (value.isEmpty()) return ""
    val needsQuotes = value.any { it.isWhitespace() || it in "\"'#`\$\\!" }
    if (!needsQuotes) return value
    return "\"" + value
        .replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("\$", "\\$")
        .replace("`", "\\`") + "\""
}

private fun String.escapeToml(): String =
    replace("\\", "\\\\").replace("\"", "\\\"")
