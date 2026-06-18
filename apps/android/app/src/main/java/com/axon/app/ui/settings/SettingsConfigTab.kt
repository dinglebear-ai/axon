package com.axon.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.AxonSensitiveTextField
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone
import tv.tootie.aurora.components.AuroraSwitch
import tv.tootie.aurora.components.AuroraTextField

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
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(11.dp)) {
        Text(
            configPathSummary(path, groups),
            color = AxonTheme.colors.textMuted,
            fontSize = 10.sp,
            fontFamily = AxonTheme.fonts.mono,
        )
        if (loading) AuroraStatusIndicator(tone = AuroraStatusTone.Syncing, label = "Loading real file values...")
        error?.let {
            ConfigAccessNotice(configAccessMessage(it), warn = it.contains("401"))
        }
        groups.forEach { group ->
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
            BoxIcon()
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
            field.env?.let { Badge("env") }
            Badge(if (explicit) "set" else "default", if (explicit) colors.success else colors.textMuted)
            if (field.kind == SettingKind.Bool) {
                AuroraSwitch(
                    checked = value.equals("true", ignoreCase = true),
                    onCheckedChange = { onChange(it.toString()) },
                    contentDescription = field.key,
                )
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
private fun CompactKnobInput(field: SettingField, value: String, onValueChange: (String) -> Unit) {
    val secret = field.kind == SettingKind.Secret
    val placeholder = when {
        secret -> "unset · secret"
        field.kind == SettingKind.List -> "comma,separated"
        field.defaultValue.isBlank() -> "unset"
        else -> field.defaultValue
    }
    val leadingIcon: (@Composable () -> Unit)? = if (secret) {
        {
            Icon(
                Icons.Rounded.Key,
                contentDescription = null,
                tint = AxonTheme.colors.textMuted,
                modifier = Modifier.size(12.dp),
            )
        }
    } else null
    if (secret) {
        AxonSensitiveTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier.fillMaxWidth(),
            placeholder = placeholder,
            compact = true,
            revealContentDescription = "Show ${field.key}",
            hideContentDescription = "Hide ${field.key}",
            contentDescription = field.key,
            leadingIcon = leadingIcon,
        )
    } else {
        AuroraTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier.fillMaxWidth(),
            placeholder = placeholder,
            singleLine = true,
            compact = true,
            contentDescription = field.key,
            leadingIcon = leadingIcon,
        )
    }
}

private fun configPathSummary(path: String, groups: List<SettingGroup>): String {
    val count = groups.sumOf { it.fields.size }
    return if (groups.any { it.section != null }) {
        "$path · $count knobs · env overrides each"
    } else {
        "$path · $count vars"
    }
}

private fun configAccessMessage(error: String): String =
    if (error.contains("Panel unlock required") || error.contains("401")) {
        "Panel unlock required. Save the panel password on Connection to load live file values."
    } else {
        "Could not load live file values. Catalog defaults are shown for now. $error"
    }

@Composable
internal fun SectionLabel(text: String) {
    Text(text.uppercase(), color = AxonTheme.colors.accentStrong, fontSize = 9.2.sp, fontWeight = FontWeight.Bold, fontFamily = AxonTheme.fonts.mono)
}

@Composable
private fun Badge(text: String, color: Color = AxonTheme.colors.textMuted) {
    Text(
        text,
        color = color,
        modifier = Modifier
            .border(1.dp, AxonTheme.colors.borderDefault.copy(alpha = 0.2f), RoundedCornerShape(4.dp))
            .padding(horizontal = 4.dp, vertical = 1.dp),
        fontSize = 8.2.sp,
        fontFamily = AxonTheme.fonts.mono,
    )
}

@Composable
private fun BoxIcon() {
    val colors = AxonTheme.colors
    androidx.compose.foundation.layout.Box(
        modifier = Modifier
            .background(colors.tint(colors.accentPrimary, 12, colors.control), RoundedCornerShape(9.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.16f), RoundedCornerShape(7.dp))
            .padding(3.dp),
        contentAlignment = Alignment.Center,
    ) {
        Icon(Icons.Rounded.Settings, contentDescription = null, tint = colors.accentStrong, modifier = Modifier.size(18.dp))
    }
}
