package com.axon.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.Link
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Slideshow
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone
import tv.tootie.aurora.components.AuroraTextField

private enum class SettingsTab(val label: String, val icon: ImageVector) {
    Connection("Connection", Icons.Rounded.Link),
    Env("Env", Icons.Rounded.Key),
    Config("config.toml", Icons.Rounded.Slideshow),
}

@Composable
fun SettingsScreen(vm: SettingsViewModel = viewModel()) {
    val settings by vm.settings.collectAsStateWithLifecycle()
    val files by vm.configFiles.collectAsStateWithLifecycle()
    val connection by vm.connection.collectAsStateWithLifecycle()
    val saveState by vm.saveState.collectAsStateWithLifecycle()

    var tab by remember { mutableStateOf(SettingsTab.Connection) }
    var serverUrl by remember(settings.serverUrl) { mutableStateOf(settings.serverUrl.value) }
    var token by remember(settings.token) { mutableStateOf(settings.token.value) }
    var panelToken by remember(settings.panelToken) { mutableStateOf(settings.panelToken.value) }
    var collection by remember(settings.collection) { mutableStateOf(settings.collection) }
    val saveLabel = when (tab) {
        SettingsTab.Connection -> "Save connection"
        SettingsTab.Env -> "Save .env"
        SettingsTab.Config -> "Save config"
    }
    val canSaveTab = when (tab) {
        SettingsTab.Connection -> true
        SettingsTab.Env -> !files.loading && files.error == null && files.envDirty.isNotEmpty()
        SettingsTab.Config -> !files.loading && files.error == null && files.configDirty.isNotEmpty()
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(AxonTheme.colors.pageBg)
            .verticalScroll(rememberScrollState())
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(7.dp), modifier = Modifier.fillMaxWidth()) {
            SettingsTab.entries.forEach { entry ->
                SettingsTabButton(entry, selected = tab == entry, modifier = Modifier.weight(1f)) {
                    tab = entry
                }
            }
        }

        when (tab) {
            SettingsTab.Connection -> ConnectionTab(
                serverUrl = serverUrl,
                token = token,
                panelToken = panelToken,
                collection = collection,
                onServerUrl = { serverUrl = it },
                onToken = { token = it },
                onPanelToken = { panelToken = it },
                onCollection = { collection = it },
                connection = connection,
            )
            SettingsTab.Env -> ConfigGroupsTab(
                path = files.envPath,
                loading = files.loading,
                error = files.error,
                groups = AxonSettingsCatalog.envGroups,
                values = files.envValues,
                explicit = files.envExplicit,
                keyFor = { _, field -> field.key },
                onChange = vm::updateEnv,
            )
            SettingsTab.Config -> ConfigGroupsTab(
                path = files.configPath,
                loading = files.loading,
                error = files.error,
                groups = AxonSettingsCatalog.configGroups,
                values = files.configValues,
                explicit = files.configExplicit,
                keyFor = { group, field -> "${group.id}.${field.key}" },
                onChange = vm::updateConfig,
            )
        }

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            AuroraButton(
                onClick = {
                    when (tab) {
                        SettingsTab.Connection -> vm.saveConnection(serverUrl, token, panelToken, collection)
                        SettingsTab.Env -> vm.saveEnvFile()
                        SettingsTab.Config -> vm.saveConfigFile()
                    }
                },
                modifier = Modifier.weight(1f),
                enabled = saveState !is SaveState.Saving && canSaveTab,
            ) {
                Text(if (saveState is SaveState.Saving) "Saving..." else saveLabel)
            }
            AuroraButton(
                onClick = {
                    if (tab == SettingsTab.Connection) vm.testConnection(serverUrl, token) else vm.refreshConfigFiles()
                },
                variant = AuroraButtonVariant.Outlined,
                modifier = Modifier.weight(1f),
            ) {
                Icon(if (tab == SettingsTab.Connection) Icons.Rounded.Check else Icons.Rounded.Refresh, contentDescription = null)
                Text(if (tab == SettingsTab.Connection) "Test" else "Reload")
            }
        }

        when (val s = saveState) {
            is SaveState.Saved -> AuroraCallout(
                message = "Settings saved. Restart Axon for file changes to affect live requests.",
                variant = AuroraCalloutVariant.Success,
            )
            is SaveState.Failed -> AuroraCallout(
                message = "Save failed: ${s.error}",
                variant = AuroraCalloutVariant.Error,
            )
            else -> {}
        }
    }
}

@Composable
private fun SettingsTabButton(tab: SettingsTab, selected: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    Row(
        modifier = modifier
            .background(if (selected) colors.tint(colors.accentPrimary, 12, colors.control) else colors.control, RoundedCornerShape(10.dp))
            .border(1.dp, if (selected) colors.tint(colors.accentPrimary, 30, colors.control) else colors.borderDefault, RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 9.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(tab.icon, contentDescription = null, tint = if (selected) colors.accentStrong else colors.textMuted, modifier = Modifier.padding(0.dp))
        Text(tab.label, color = if (selected) colors.accentStrong else colors.textMuted, fontSize = 12.5.sp, fontWeight = FontWeight.SemiBold, fontFamily = AxonTheme.fonts.body)
    }
}

@Composable
private fun ConnectionTab(
    serverUrl: String,
    token: String,
    panelToken: String,
    collection: String,
    onServerUrl: (String) -> Unit,
    onToken: (String) -> Unit,
    onPanelToken: (String) -> Unit,
    onCollection: (String) -> Unit,
    connection: TestConnectionState,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        SectionLabel("Connection")
        AuroraTextField(serverUrl, onServerUrl, label = "Server", modifier = Modifier.fillMaxWidth())
        if (serverUrl.startsWith("http://")) {
            AuroraCallout(
                message = "Cleartext HTTP is in use. Prefer HTTPS for non-Tailscale servers.",
                variant = AuroraCalloutVariant.Warn,
            )
        }
        AuroraTextField(token, onToken, label = "Bearer token", modifier = Modifier.fillMaxWidth(), visualTransformation = PasswordVisualTransformation())
        AuroraTextField(panelToken, onPanelToken, label = "Panel token", modifier = Modifier.fillMaxWidth(), visualTransformation = PasswordVisualTransformation())
        AuroraTextField(collection, onCollection, label = "Collection", modifier = Modifier.fillMaxWidth())
        when (val c = connection) {
            is TestConnectionState.Testing -> AuroraStatusIndicator(tone = AuroraStatusTone.Syncing, label = "Testing...")
            is TestConnectionState.Ok -> AuroraStatusIndicator(tone = AuroraStatusTone.Online, label = "Connected")
            is TestConnectionState.Failed -> AuroraStatusIndicator(tone = AuroraStatusTone.Error, label = c.error)
            else -> {}
        }
    }
}

@Composable
private fun ConfigGroupsTab(
    path: String,
    loading: Boolean,
    error: String?,
    groups: List<SettingGroup>,
    values: Map<String, String>,
    explicit: Set<String>,
    keyFor: (SettingGroup, SettingField) -> String,
    onChange: (String, String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(path, color = AxonTheme.colors.textMuted, fontSize = 11.sp, fontFamily = AxonTheme.fonts.mono)
        if (loading) AuroraStatusIndicator(tone = AuroraStatusTone.Syncing, label = "Loading real file values...")
        error?.let {
            AuroraCallout(
                message = "Config load failed: $it",
                variant = AuroraCalloutVariant.Error,
            )
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
private fun SettingGroupCard(group: SettingGroup, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(9.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            BoxIcon()
            group.section?.let {
                Text(it, color = AxonTheme.colors.accentStrong, fontSize = 12.5.sp, fontFamily = AxonTheme.fonts.mono)
            }
            Text(group.label, color = AxonTheme.colors.textPrimary, fontSize = 15.sp, fontWeight = FontWeight.ExtraBold, fontFamily = AxonTheme.fonts.display, modifier = Modifier.weight(1f))
            Text("${group.fields.size}", color = AxonTheme.colors.textMuted, fontSize = 10.5.sp, fontFamily = AxonTheme.fonts.mono)
        }
        Text(group.note, color = AxonTheme.colors.textMuted, fontSize = 12.sp, lineHeight = 18.sp, fontFamily = AxonTheme.fonts.body)
        content()
    }
}

@Composable
private fun SettingEditor(field: SettingField, value: String, explicit: Boolean, onChange: (String) -> Unit) {
    val colors = AxonTheme.colors
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(colors.control, RoundedCornerShape(12.dp))
            .border(1.dp, colors.borderDefault, RoundedCornerShape(12.dp))
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            Text(field.key, color = colors.textPrimary, fontSize = 12.5.sp, fontWeight = FontWeight.Bold, fontFamily = AxonTheme.fonts.mono, modifier = Modifier.weight(1f))
            field.env?.let { Badge("env") }
            Badge(if (explicit) "set" else "default", if (explicit) colors.success else colors.textMuted)
        }
        if (field.kind == SettingKind.Bool) {
            ToggleRow(value.equals("true", ignoreCase = true)) { onChange(it.toString()) }
        } else {
            AuroraTextField(
                value = value,
                onValueChange = onChange,
                label = field.defaultValue.ifBlank { "unset" },
                modifier = Modifier.fillMaxWidth(),
                visualTransformation = if (field.kind == SettingKind.Secret) PasswordVisualTransformation() else VisualTransformation.None,
            )
        }
        Text(field.desc, color = colors.textMuted, fontSize = 11.5.sp, lineHeight = 16.sp, fontFamily = AxonTheme.fonts.body)
    }
}

@Composable
private fun ToggleRow(on: Boolean, onChange: (Boolean) -> Unit) {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(if (on) "true" else "false", color = colors.textPrimary, fontFamily = AxonTheme.fonts.mono, fontSize = 13.sp)
        Text(
            if (on) "ON" else "OFF",
            color = if (on) colors.accentStrong else colors.textMuted,
            modifier = Modifier
                .background(if (on) colors.tint(colors.accentPrimary, 14, colors.control) else colors.pageBg, RoundedCornerShape(999.dp))
                .border(1.dp, if (on) colors.tint(colors.accentPrimary, 30, colors.control) else colors.borderDefault, RoundedCornerShape(999.dp))
                .clickable { onChange(!on) }
                .padding(horizontal = 14.dp, vertical = 6.dp),
            fontFamily = AxonTheme.fonts.mono,
            fontSize = 11.sp,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(text.uppercase(), color = AxonTheme.colors.accentStrong, fontSize = 10.sp, fontWeight = FontWeight.Bold, fontFamily = AxonTheme.fonts.mono)
}

@Composable
private fun Badge(text: String, color: Color = AxonTheme.colors.textMuted) {
    Text(
        text,
        color = color,
        modifier = Modifier
            .border(1.dp, AxonTheme.colors.borderDefault, RoundedCornerShape(4.dp))
            .padding(horizontal = 5.dp, vertical = 1.dp),
        fontSize = 9.sp,
        fontFamily = AxonTheme.fonts.mono,
    )
}

@Composable
private fun BoxIcon() {
    val colors = AxonTheme.colors
    androidx.compose.foundation.layout.Box(
        modifier = Modifier
            .background(colors.tint(colors.accentPrimary, 12, colors.control), RoundedCornerShape(9.dp))
            .border(1.dp, colors.borderDefault, RoundedCornerShape(9.dp))
            .padding(6.dp),
        contentAlignment = Alignment.Center,
    ) {
        Icon(Icons.Rounded.Settings, contentDescription = null, tint = colors.accentStrong)
    }
}
