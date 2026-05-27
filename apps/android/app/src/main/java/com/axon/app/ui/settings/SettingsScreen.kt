package com.axon.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone
import tv.tootie.aurora.components.AuroraTextField

@Composable
fun SettingsScreen(vm: SettingsViewModel = viewModel()) {
    val settings by vm.settings.collectAsStateWithLifecycle()
    val connection by vm.connection.collectAsStateWithLifecycle()
    val saveState by vm.saveState.collectAsStateWithLifecycle()

    var serverUrl  by remember(settings.serverUrl)  { mutableStateOf(settings.serverUrl.value) }
    var token      by remember(settings.token)      { mutableStateOf(settings.token.value) }
    var collection by remember(settings.collection) { mutableStateOf(settings.collection) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("Settings", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()

        // Server Configuration section
        Text(
            "Server Configuration",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        AuroraTextField(
            value = serverUrl,
            onValueChange = { serverUrl = it },
            label = "Server URL",
            modifier = Modifier.fillMaxWidth(),
        )

        // Cleartext HTTP warning
        if (serverUrl.startsWith("http://")) {
            AuroraCallout(
                message = "Cleartext HTTP is in use. Consider switching to HTTPS for non-Tailscale servers.",
                variant = AuroraCalloutVariant.Warn,
                modifier = Modifier.fillMaxWidth(),
            )
        }

        AuroraTextField(
            value = token,
            onValueChange = { token = it },
            label = "API Token",
            modifier = Modifier.fillMaxWidth(),
            visualTransformation = PasswordVisualTransformation(),
        )

        AuroraTextField(
            value = collection,
            onValueChange = { collection = it },
            label = "Collection",
            modifier = Modifier.fillMaxWidth(),
        )

        AuroraSeparator()

        // Actions section
        Text(
            "Actions",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            AuroraButton(
                onClick = { vm.saveSettings(serverUrl, token, collection) },
                modifier = Modifier.weight(1f),
                enabled = saveState !is SaveState.Saving,
            ) {
                Text(if (saveState is SaveState.Saving) "Saving…" else "Save")
            }
            AuroraButton(
                onClick = { vm.testConnection(serverUrl, token) },
                variant = AuroraButtonVariant.Outlined,
                modifier = Modifier.weight(1f),
                enabled = connection !is ConnectionState.Testing,
            ) {
                Text(if (connection is ConnectionState.Testing) "Testing…" else "Test")
            }
        }

        AuroraSeparator()

        // Status section
        Text(
            "Status",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        // Save result feedback
        when (val s = saveState) {
            is SaveState.Saved ->
                AuroraCallout(
                    message = "Settings saved successfully.",
                    variant = AuroraCalloutVariant.Success,
                    modifier = Modifier.fillMaxWidth(),
                )
            is SaveState.Failed ->
                AuroraCallout(
                    message = "Save failed: ${s.error}",
                    variant = AuroraCalloutVariant.Error,
                    modifier = Modifier.fillMaxWidth(),
                )
            else -> {}
        }

        // Connection status
        when (val c = connection) {
            is ConnectionState.Testing ->
                AuroraStatusIndicator(
                    tone = AuroraStatusTone.Syncing,
                    label = "Testing…",
                )
            is ConnectionState.Ok -> {
                AuroraStatusIndicator(
                    tone = AuroraStatusTone.Online,
                    label = "Connected",
                )
                c.warning?.let { warning ->
                    AuroraCallout(
                        message = warning,
                        variant = AuroraCalloutVariant.Warn,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
            is ConnectionState.Failed ->
                AuroraStatusIndicator(
                    tone = AuroraStatusTone.Error,
                    label = c.error,
                )
            else -> {}
        }
    }
}
