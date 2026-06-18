package com.axon.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Article
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.Cached
import androidx.compose.material.icons.rounded.Code
import androidx.compose.material.icons.rounded.DataObject
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.Layers
import androidx.compose.material.icons.rounded.Memory
import androidx.compose.material.icons.rounded.Public
import androidx.compose.material.icons.rounded.Psychology
import androidx.compose.material.icons.rounded.QuestionAnswer
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Security
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material.icons.rounded.TravelExplore
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone

@Composable
internal fun ConfigGroupsTab(
    path: String,
    loading: Boolean,
    error: String?,
    groups: List<SettingGroup>,
    values: Map<String, String>,
    explicit: Set<String>,
    keyFor: (SettingGroup, SettingField) -> String,
    onChange: (String, String) -> Unit,
    searchQuery: String,
    onSearchQueryChange: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val filteredGroups = remember(groups, values, searchQuery) {
        filterGroups(groups, values, keyFor, searchQuery)
    }
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(11.dp)) {
        SettingsSearchField(
            value = searchQuery,
            onValueChange = onSearchQueryChange,
        )
        Text(
            configPathSummary(path, groups, filteredGroups, searchQuery),
            color = AxonTheme.colors.textMuted,
            fontSize = 10.sp,
            fontFamily = AxonTheme.fonts.mono,
        )
        if (loading) AuroraStatusIndicator(tone = AuroraStatusTone.Syncing, label = "Loading real file values...")
        error?.let {
            ConfigAccessNotice(configAccessMessage(it), warn = it.contains("401"))
        }
        filteredGroups.forEach { group ->
            SettingGroupCard(group = group) {
                group.fields.forEach { field ->
                    val key = keyFor(group, field)
                    SettingEditor(
                        field = field,
                        value = values[key].orEmpty(),
                        explicit = key in explicit,
                        onChange = { onChange(key, it) },
                    )
                }
            }
        }
        if (filteredGroups.isEmpty()) {
            Text(
                "No settings match \"$searchQuery\"",
                color = AxonTheme.colors.textMuted,
                fontSize = 11.sp,
                fontFamily = AxonTheme.fonts.body,
                modifier = Modifier.padding(vertical = 24.dp),
            )
        }
    }
}

@Composable
private fun SettingsSearchField(value: String, onValueChange: (String) -> Unit) {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(42.dp)
            .clip(RoundedCornerShape(9.dp))
            .background(colors.control.copy(alpha = 0.46f), RoundedCornerShape(9.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.22f), RoundedCornerShape(9.dp))
            .padding(horizontal = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(Icons.Rounded.Search, contentDescription = null, tint = colors.textMuted, modifier = Modifier.size(15.dp))
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            singleLine = true,
            textStyle = TextStyle(
                color = colors.textPrimary,
                fontSize = 11.3.sp,
                fontFamily = AxonTheme.fonts.body,
            ),
            modifier = Modifier.weight(1f),
            decorationBox = { inner ->
                Box(modifier = Modifier.fillMaxWidth()) {
                    if (value.isBlank()) {
                        Text("Search settings", color = colors.textMuted, fontSize = 11.3.sp, fontFamily = AxonTheme.fonts.body)
                    }
                    inner()
                }
            },
        )
    }
}

@Composable
private fun ConfigAccessNotice(message: String, warn: Boolean) {
    val colors = AxonTheme.colors
    val tone = if (warn) colors.warn else colors.error
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(colors.tint(tone, 5, colors.pageBg), RoundedCornerShape(8.dp))
            .border(1.dp, colors.tint(tone, 22, colors.pageBg), RoundedCornerShape(8.dp))
            .padding(horizontal = 10.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            Icons.Rounded.WarningAmber,
            contentDescription = null,
            tint = tone,
            modifier = Modifier.size(15.dp).padding(top = 1.dp),
        )
        Text(
            humanizeJsonFragmentText(message),
            color = colors.textPrimary,
            fontSize = 9.8.sp,
            lineHeight = 13.3.sp,
            fontFamily = AxonTheme.fonts.body,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun SettingGroupCard(group: SettingGroup, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(7.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            BoxIcon(group.icon)
            group.section?.let {
                Text(it, color = AxonTheme.colors.accentStrong, fontSize = 9.5.sp, fontFamily = AxonTheme.fonts.mono)
            }
            Text(group.label, color = AxonTheme.colors.textPrimary, fontSize = 11.7.sp, fontWeight = FontWeight.ExtraBold, fontFamily = AxonTheme.fonts.display, modifier = Modifier.weight(1f))
            Text("${group.fields.size} ${if (group.section == null) "vars" else "knobs"}", color = AxonTheme.colors.textMuted, fontSize = 9.5.sp, fontFamily = AxonTheme.fonts.mono)
        }
        Text(group.note, color = AxonTheme.colors.textMuted.copy(alpha = 0.78f), fontSize = 10.1.sp, lineHeight = 13.9.sp, fontFamily = AxonTheme.fonts.body)
        content()
    }
}

@Composable
private fun SettingEditor(field: SettingField, value: String, explicit: Boolean, onChange: (String) -> Unit) {
    val colors = AxonTheme.colors
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(colors.control.copy(alpha = 0.035f), RoundedCornerShape(8.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.08f), RoundedCornerShape(8.dp))
            .padding(horizontal = 11.dp, vertical = 9.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                field.key,
                color = colors.textPrimary,
                fontSize = if (field.env == null) 10.1.sp else 10.4.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = AxonTheme.fonts.mono,
                modifier = Modifier.weight(1f),
            )
            field.env?.let { Badge("env", colors.accentStrong) }
            Badge(if (explicit) "set" else "default", if (explicit) colors.success else colors.textMuted)
            if (field.kind == SettingKind.Bool) {
                MiniToggle(value.equals("true", ignoreCase = true)) { onChange(it.toString()) }
            }
        }
        if (field.kind != SettingKind.Bool) {
            CompactKnobInput(
                field = field,
                value = value,
                onValueChange = onChange,
            )
        }
        Text(field.desc, color = colors.textMuted.copy(alpha = 0.76f), fontSize = 9.9.sp, lineHeight = 13.5.sp, fontFamily = AxonTheme.fonts.body)
    }
}

@Composable
private fun MiniToggle(on: Boolean, onChange: (Boolean) -> Unit) {
    val colors = AxonTheme.colors
    Box(
        modifier = Modifier
            .width(38.dp)
            .height(20.dp)
            .background(if (on) colors.accentDeep else colors.control, RoundedCornerShape(999.dp))
            .border(1.dp, if (on) colors.accentPrimary else colors.borderDefault, RoundedCornerShape(999.dp))
            .clickable { onChange(!on) },
    ) {
        Box(
            modifier = Modifier
                .offset(x = if (on) 18.dp else 2.dp, y = 2.dp)
                .size(14.dp)
                .background(if (on) colors.accentStrong else colors.textMuted, RoundedCornerShape(999.dp)),
        )
    }
}

@Composable
private fun CompactKnobInput(field: SettingField, value: String, onValueChange: (String) -> Unit) {
    var reveal by remember { mutableStateOf(false) }
    val secret = field.kind == SettingKind.Secret
    val placeholder = when {
        secret -> "unset · secret"
        field.kind == SettingKind.List -> "comma,separated"
        field.defaultValue.isBlank() -> "unset"
        else -> field.defaultValue
    }
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(38.dp)
            .background(colors.control.copy(alpha = 0.5f), RoundedCornerShape(8.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.18f), RoundedCornerShape(8.dp))
            .padding(start = if (secret) 9.dp else 10.dp, end = if (secret) 5.dp else 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        if (secret) {
            Icon(Icons.Rounded.Key, contentDescription = null, tint = colors.textMuted, modifier = Modifier.size(12.dp))
        }
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            singleLine = true,
            visualTransformation = if (secret && !reveal) PasswordVisualTransformation() else VisualTransformation.None,
            textStyle = TextStyle(
                color = colors.textPrimary,
                fontSize = 10.9.sp,
                fontFamily = AxonTheme.fonts.mono,
            ),
            modifier = Modifier.weight(1f),
            decorationBox = { inner ->
                Box(modifier = Modifier.fillMaxWidth()) {
                    if (value.isBlank()) {
                        Text(placeholder, color = colors.textMuted, fontSize = 10.9.sp, fontFamily = AxonTheme.fonts.mono)
                    }
                    inner()
                }
            },
        )
        if (secret) {
            Icon(
                if (reveal) Icons.Rounded.VisibilityOff else Icons.Rounded.Visibility,
                contentDescription = null,
                tint = if (reveal) colors.accentStrong else colors.textMuted,
                modifier = Modifier
                    .size(30.dp)
                    .clip(RoundedCornerShape(6.dp))
                    .clickable { reveal = !reveal }
                    .padding(8.dp),
            )
        }
    }
}

private fun configPathSummary(
    path: String,
    groups: List<SettingGroup>,
    filteredGroups: List<SettingGroup>,
    query: String,
): String {
    val count = groups.sumOf { it.fields.size }
    val filteredCount = filteredGroups.sumOf { it.fields.size }
    val base = if (groups.any { it.section != null }) {
        "$path · $count knobs · env overrides each"
    } else {
        "$path · $count vars"
    }
    return if (query.isBlank()) base else "$base · showing $filteredCount"
}

private fun configAccessMessage(error: String): String =
    if (error.contains("Panel unlock required") || error.contains("401")) {
        "Axon API token required. Save the bearer token on Connection to load live file values."
    } else {
        "Could not load live file values. Catalog defaults are shown for now. $error"
    }

@Composable
internal fun SectionLabel(text: String) {
    Text(text.uppercase(), color = AxonTheme.colors.accentStrong, fontSize = 9.2.sp, fontWeight = FontWeight.Bold, fontFamily = AxonTheme.fonts.mono)
}

@Composable
private fun Badge(text: String, color: Color = AxonTheme.colors.textMuted) {
    val colors = AxonTheme.colors
    Text(
        text.uppercase(),
        color = color,
        modifier = Modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(color, 9, colors.panelMedium), RoundedCornerShape(999.dp))
            .border(1.dp, color.copy(alpha = 0.32f), RoundedCornerShape(999.dp))
            .padding(horizontal = 7.dp, vertical = 3.dp),
        fontSize = 8.sp,
        fontWeight = FontWeight.Bold,
        fontFamily = AxonTheme.fonts.body,
    )
}

@Composable
private fun BoxIcon(iconName: String) {
    val colors = AxonTheme.colors
    androidx.compose.foundation.layout.Box(
        modifier = Modifier
            .background(colors.tint(colors.accentPrimary, 12, colors.control), RoundedCornerShape(9.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.16f), RoundedCornerShape(7.dp))
            .padding(3.dp),
        contentAlignment = Alignment.Center,
    ) {
        Icon(groupIcon(iconName), contentDescription = null, tint = colors.accentStrong, modifier = Modifier.size(18.dp))
    }
}

private fun groupIcon(name: String): ImageVector = when (name) {
    "server", "database" -> Icons.Rounded.Storage
    "shield" -> Icons.Rounded.Security
    "key" -> Icons.Rounded.Key
    "brain" -> Icons.Rounded.Psychology
    "globe" -> Icons.Rounded.Public
    "file" -> Icons.AutoMirrored.Rounded.Article
    "layers" -> Icons.Rounded.Layers
    "search" -> Icons.Rounded.Search
    "ask" -> Icons.Rounded.QuestionAnswer
    "zap" -> Icons.Rounded.Bolt
    "activity" -> Icons.Rounded.Memory
    "scrape" -> Icons.Rounded.TravelExplore
    "braces" -> Icons.Rounded.Code
    "clock" -> Icons.Rounded.Schedule
    "cache" -> Icons.Rounded.Cached
    "payload" -> Icons.Rounded.DataObject
    else -> Icons.Rounded.Settings
}

private fun filterGroups(
    groups: List<SettingGroup>,
    values: Map<String, String>,
    keyFor: (SettingGroup, SettingField) -> String,
    rawQuery: String,
): List<SettingGroup> {
    val query = rawQuery.trim().lowercase()
    if (query.isBlank()) return groups
    return groups.mapNotNull { group ->
        val groupMatches = listOfNotNull(group.id, group.section, group.label, group.note, group.icon)
            .any { it.lowercase().contains(query) }
        val fields = group.fields.filter { field ->
            val key = keyFor(group, field)
            groupMatches || listOf(
                field.key,
                field.desc,
                field.env.orEmpty(),
                field.defaultValue,
                values[key].orEmpty(),
            ).any { it.lowercase().contains(query) }
        }
        if (fields.isEmpty()) null else group.copy(fields = fields)
    }
}
