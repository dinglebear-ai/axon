package com.axon.app.ui.settings

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.clip
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import kotlinx.coroutines.launch

private enum class SettingsTab(val label: String, val shortLabel: String, val icon: ImageVector) {
    Connection("Connection", "Conn", Icons.Rounded.Link),
    Env("Env", "Env", Icons.Rounded.Key),
    Config("Config", "Config", Icons.Rounded.Slideshow),
}

@Composable
fun SettingsScreen(vm: SettingsViewModel = viewModel()) {
    val settings by vm.settings.collectAsStateWithLifecycle()
    val files by vm.configFiles.collectAsStateWithLifecycle()
    val collections by vm.collections.collectAsStateWithLifecycle()
    val connection by vm.connection.collectAsStateWithLifecycle()
    val saveState by vm.saveState.collectAsStateWithLifecycle()
    val draftAuthMode by vm.draftAuthMode.collectAsStateWithLifecycle()
    val oauthStatus by vm.oauthStatus.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    val oauthLauncher = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        if (result.data == null) vm.cancelOAuthSignIn() else vm.completeOAuthSignIn(result.data)
    }

    var tab by remember { mutableStateOf(SettingsTab.Connection) }
    var serverUrl by remember(settings.serverUrl) { mutableStateOf(settings.serverUrl.value) }
    var token by remember(settings.token) { mutableStateOf(settings.token.value) }
    var collection by remember(settings.collection) { mutableStateOf(settings.collection) }
    var settingsSearch by remember { mutableStateOf("") }
    val canSaveTab = when (tab) {
        SettingsTab.Connection -> true
        SettingsTab.Env -> !files.loading && files.error == null && files.envDirty.isNotEmpty()
        SettingsTab.Config -> !files.loading && files.error == null && files.configDirty.isNotEmpty()
    }
    val saveFeedback: Pair<String, SettingsFeedbackKind>? = when (val s = saveState) {
        is SaveState.Saved -> "Settings saved. Restart Axon for file changes to affect live requests." to SettingsFeedbackKind.Success
        is SaveState.Failed -> humanizeJsonFragmentText("Save failed: ${s.error}") to SettingsFeedbackKind.Error
        else -> null
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(AxonTheme.colors.pageBg),
    ) {
        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 8.dp, vertical = 10.dp)
                .padding(bottom = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .widthIn(max = 460.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(AxonTheme.colors.panelMedium.copy(alpha = 0.28f), RoundedCornerShape(10.dp))
                    .border(1.dp, AxonTheme.colors.borderDefault.copy(alpha = 0.12f), RoundedCornerShape(10.dp))
                    .padding(3.dp),
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
                    collection = collection,
                    onServerUrl = { serverUrl = it },
                    onToken = { token = it },
                    onCollection = { collection = it },
                    authMode = draftAuthMode,
                    oauthStatus = oauthStatus,
                    onAuthMode = vm::setDraftAuthMode,
                    onBeginOAuth = {
                        scope.launch {
                            vm.beginOAuthSignIn(serverUrl).fold(
                                onSuccess = oauthLauncher::launch,
                                onFailure = { },
                            )
                        }
                    },
                    onSignOutOAuth = vm::signOutOAuth,
                    collections = collections,
                    onRefreshCollections = vm::refreshCollections,
                    connection = connection,
                    modifier = Modifier
                        .fillMaxWidth()
                        .widthIn(max = 460.dp),
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
                    searchQuery = settingsSearch,
                    onSearchQueryChange = { settingsSearch = it },
                    modifier = Modifier
                        .fillMaxWidth()
                        .widthIn(max = 600.dp),
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
                    searchQuery = settingsSearch,
                    onSearchQueryChange = { settingsSearch = it },
                    modifier = Modifier
                        .fillMaxWidth()
                        .widthIn(max = 600.dp),
                )
            }
        }
        SettingsActionDock(
            feedback = saveFeedback,
            primaryLabel = if (saveState is SaveState.Saving) "Saving..." else "Save",
            primaryEnabled = saveState !is SaveState.Saving && canSaveTab,
            onPrimary = {
                when (tab) {
                    SettingsTab.Connection -> vm.saveConnection(serverUrl, token, collection)
                    SettingsTab.Env -> vm.saveEnvFile()
                    SettingsTab.Config -> vm.saveConfigFile()
                }
            },
            secondaryLabel = if (tab == SettingsTab.Connection) "Test" else "Reload",
            secondaryIcon = if (tab == SettingsTab.Connection) Icons.Rounded.Check else Icons.Rounded.Refresh,
            onSecondary = {
                if (tab == SettingsTab.Connection) vm.testConnection(serverUrl, token) else vm.refreshConfigFiles()
            },
            modifier = Modifier.fillMaxWidth(),
        )
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
            .clip(RoundedCornerShape(8.dp))
            .background(if (selected) colors.tint(colors.accentPrimary, 7, colors.pageBg) else colors.control.copy(alpha = 0.01f), RoundedCornerShape(8.dp))
            .border(1.dp, if (selected) colors.tint(colors.accentPrimary, 20, colors.pageBg) else colors.borderDefault.copy(alpha = 0.015f), RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .height(46.dp)
            .padding(horizontal = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(tab.icon, contentDescription = null, tint = if (selected) colors.accentStrong else colors.textMuted.copy(alpha = 0.72f), modifier = Modifier.size(15.dp))
        Text(
            tab.shortLabel,
            color = if (selected) colors.accentStrong else colors.textMuted,
            fontSize = 13.sp,
            lineHeight = 17.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        count?.let {
            Text(
                it.toString(),
                modifier = Modifier
                    .width(32.dp)
                    .height(22.dp)
                    .clip(RoundedCornerShape(999.dp))
                    .background(colors.control.copy(alpha = if (selected) 0.34f else 0.18f))
                    .border(1.dp, colors.borderDefault.copy(alpha = if (selected) 0.18f else 0.08f), RoundedCornerShape(999.dp)),
                color = colors.textMuted.copy(alpha = 0.78f),
                fontSize = 10.4.sp,
                lineHeight = 22.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Clip,
                softWrap = false,
                textAlign = TextAlign.Center,
            )
        }
    }
}
