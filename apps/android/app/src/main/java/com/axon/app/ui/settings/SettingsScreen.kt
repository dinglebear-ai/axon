package com.axon.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.Link
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Slideshow
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.AxonSensitiveTextField
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import kotlinx.collections.immutable.toImmutableList
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone
import tv.tootie.aurora.components.AuroraTabs
import tv.tootie.aurora.components.AuroraTextField

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
        AuroraTabs(
            tabs = SettingsTab.entries.map { it.shortLabel }.toImmutableList(),
            selectedIndex = SettingsTab.entries.indexOf(tab),
            onTabSelected = { index -> tab = SettingsTab.entries[index] },
            modifier = Modifier
                .fillMaxWidth(0.90f)
                .widthIn(max = 380.dp),
            compact = true,
        )

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
                loading = saveState is SaveState.Saving,
            ) {
                Text(if (saveState is SaveState.Saving) "Saving..." else saveLabel)
            }
            AuroraButton(
                onClick = {
                    if (tab == SettingsTab.Connection) vm.testConnection(serverUrl, token) else vm.refreshConfigFiles()
                },
                modifier = Modifier.weight(1f),
                variant = AuroraButtonVariant.Outlined,
                leadingIcon = {
                    Icon(
                        if (tab == SettingsTab.Connection) Icons.Rounded.Check else Icons.Rounded.Refresh,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                    )
                },
            ) {
                Text(if (tab == SettingsTab.Connection) "Test" else "Reload")
            }
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
        CompactSettingField("Bearer token", token, onToken, sensitive = true)
        CompactSettingField("Panel password", panelToken, onPanelToken, sensitive = true)
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
    sensitive: Boolean = false,
) {
    val colors = AxonTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
        Text(label, color = colors.textMuted.copy(alpha = 0.8f), fontSize = 10.8.sp, fontFamily = AxonTheme.fonts.body)
        if (sensitive) {
            AxonSensitiveTextField(
                value = value,
                onValueChange = onValueChange,
                compact = true,
                placeholder = "unset",
                revealContentDescription = "Show $label",
                hideContentDescription = "Hide $label",
                contentDescription = label,
                modifier = Modifier.fillMaxWidth(),
            )
        } else {
            AuroraTextField(
                value = value,
                onValueChange = onValueChange,
                singleLine = true,
                compact = true,
                label = null,
                placeholder = "unset",
                contentDescription = label,
                visualTransformation = visualTransformation,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}
