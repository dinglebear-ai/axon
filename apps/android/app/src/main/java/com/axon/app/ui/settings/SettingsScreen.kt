package com.axon.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.Link
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Slideshow
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone

private enum class SettingsTab(val label: String, val shortLabel: String, val icon: ImageVector) {
    Connection("Connection", "Conn", Icons.Rounded.Link),
    Env("Env", ".env", Icons.Rounded.Key),
    Config("config.toml", "TOML", Icons.Rounded.Slideshow),
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
        SettingsTab.Connection -> "Save"
        SettingsTab.Env -> "Save"
        SettingsTab.Config -> "Save"
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
            .padding(horizontal = 12.dp, vertical = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(7.dp),
            modifier = Modifier
                .fillMaxWidth(0.90f)
                .widthIn(max = 380.dp),
        ) {
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
                modifier = Modifier
                    .fillMaxWidth(0.90f)
                    .widthIn(max = 380.dp),
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
                modifier = Modifier
                    .fillMaxWidth(0.90f)
                    .widthIn(max = 380.dp),
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
                modifier = Modifier
                    .fillMaxWidth(0.90f)
                    .widthIn(max = 380.dp),
            )
        }

        Row(
            horizontalArrangement = Arrangement.spacedBy(7.dp),
            modifier = Modifier
                .fillMaxWidth(0.90f)
                .widthIn(max = 380.dp),
        ) {
            CompactActionButton(
                label = if (saveState is SaveState.Saving) "Saving..." else saveLabel,
                onClick = {
                    when (tab) {
                        SettingsTab.Connection -> vm.saveConnection(serverUrl, token, panelToken, collection)
                        SettingsTab.Env -> vm.saveEnvFile()
                        SettingsTab.Config -> vm.saveConfigFile()
                    }
                },
                modifier = Modifier.weight(1f),
                enabled = saveState !is SaveState.Saving && canSaveTab,
            )
            CompactActionButton(
                label = if (tab == SettingsTab.Connection) "Test" else "Reload",
                onClick = {
                    if (tab == SettingsTab.Connection) vm.testConnection(serverUrl, token) else vm.refreshConfigFiles()
                },
                modifier = Modifier.weight(1f),
                outlined = true,
                icon = if (tab == SettingsTab.Connection) Icons.Rounded.Check else Icons.Rounded.Refresh,
            )
        }

        when (val s = saveState) {
            is SaveState.Saved -> AuroraCallout(
                message = "Settings saved. Restart Axon for file changes to affect live requests.",
                variant = AuroraCalloutVariant.Success,
            )
            is SaveState.Failed -> AuroraCallout(
                message = humanizeJsonFragmentText("Save failed: ${s.error}"),
                variant = AuroraCalloutVariant.Error,
            )
            else -> {}
        }
    }
}

@Composable
private fun SettingsTabButton(tab: SettingsTab, selected: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    val count = when (tab) {
        SettingsTab.Connection -> null
        SettingsTab.Env -> AxonSettingsCatalog.envGroups.sumOf { it.fields.size }
        SettingsTab.Config -> AxonSettingsCatalog.configGroups.sumOf { it.fields.size }
    }
    Row(
        modifier = modifier
            .background(if (selected) colors.tint(colors.accentPrimary, 5, colors.pageBg) else colors.control.copy(alpha = 0.04f), RoundedCornerShape(8.dp))
            .border(1.dp, if (selected) colors.tint(colors.accentPrimary, 22, colors.pageBg) else colors.borderDefault.copy(alpha = 0.1f), RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .height(42.dp)
            .padding(horizontal = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(tab.icon, contentDescription = null, tint = if (selected) colors.accentStrong else colors.textMuted.copy(alpha = 0.72f), modifier = Modifier.size(14.dp))
        Text(
            tab.shortLabel,
            color = if (selected) colors.accentStrong else colors.textMuted,
            fontSize = 11.4.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        count?.let {
            Text(it.toString(), color = colors.textMuted.copy(alpha = 0.72f), fontSize = 9.5.sp, fontFamily = AxonTheme.fonts.mono)
        }
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
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(9.dp)) {
        SectionLabel("Connection")
        CompactSettingField("Server", serverUrl, onServerUrl)
        if (serverUrl.startsWith("http://")) {
            AuroraCallout(
                message = "Cleartext HTTP is in use. Prefer HTTPS for non-Tailscale servers.",
                variant = AuroraCalloutVariant.Warn,
            )
        }
        CompactSettingField("Bearer token", token, onToken, visualTransformation = PasswordVisualTransformation())
        CompactSettingField("Panel password", panelToken, onPanelToken, visualTransformation = PasswordVisualTransformation())
        CompactSettingField("Collection", collection, onCollection)
        when (val c = connection) {
            is TestConnectionState.Testing -> AuroraStatusIndicator(tone = AuroraStatusTone.Syncing, label = "Testing...")
            is TestConnectionState.Ok -> AuroraStatusIndicator(tone = AuroraStatusTone.Online, label = "Connected")
            is TestConnectionState.Failed -> AuroraStatusIndicator(tone = AuroraStatusTone.Error, label = humanizeJsonFragmentText(c.error))
            else -> {}
        }
    }
}

@Composable
private fun CompactSettingField(
    label: String,
    value: String,
    onValueChange: (String) -> Unit,
    visualTransformation: VisualTransformation = VisualTransformation.None,
) {
    val colors = AxonTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
        Text(label, color = colors.textMuted.copy(alpha = 0.8f), fontSize = 10.8.sp, fontFamily = AxonTheme.fonts.body)
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            singleLine = true,
            visualTransformation = visualTransformation,
            textStyle = TextStyle(
                color = colors.textPrimary,
                fontSize = 11.8.sp,
                fontFamily = AxonTheme.fonts.mono,
            ),
            modifier = Modifier
                .fillMaxWidth()
                .height(40.dp)
                .background(colors.control.copy(alpha = 0.5f), RoundedCornerShape(8.dp))
                .border(1.dp, colors.borderDefault.copy(alpha = 0.22f), RoundedCornerShape(8.dp))
                .padding(horizontal = 12.dp),
            decorationBox = { inner ->
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxSize()) {
                    Box(modifier = Modifier.weight(1f)) {
                        if (value.isBlank()) {
                            Text("unset", color = colors.textMuted, fontSize = 11.8.sp, fontFamily = AxonTheme.fonts.mono)
                        }
                        inner()
                    }
                }
            },
        )
    }
}

@Composable
private fun CompactActionButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    outlined: Boolean = false,
    icon: ImageVector? = null,
) {
    val colors = AxonTheme.colors
    val bg = if (outlined) colors.pageBg else colors.accentPrimary
    val fg = if (outlined) colors.textMuted else colors.onAccentFg
    Row(
        modifier = modifier
            .height(42.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(if (enabled) bg else colors.control, RoundedCornerShape(8.dp))
            .border(1.dp, if (outlined) colors.borderStrong.copy(alpha = 0.56f) else colors.accentPrimary, RoundedCornerShape(8.dp))
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 12.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        icon?.let {
            Icon(it, contentDescription = null, tint = fg, modifier = Modifier.size(14.dp).padding(end = 6.dp))
        }
        Text(
            label,
            color = fg,
            fontSize = 12.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

