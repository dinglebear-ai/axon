package com.axon.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Error
import androidx.compose.material3.Icon
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
import tv.tootie.aurora.components.AuroraTextField

@Composable
fun SettingsScreen(vm: SettingsViewModel = viewModel()) {
    val settings by vm.settings.collectAsStateWithLifecycle()
    val connection by vm.connection.collectAsStateWithLifecycle()

    var serverUrl  by remember(settings.serverUrl)  { mutableStateOf(settings.serverUrl) }
    var token      by remember(settings.token)      { mutableStateOf(settings.token) }
    var collection by remember(settings.collection) { mutableStateOf(settings.collection) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("Settings", style = MaterialTheme.typography.headlineMedium)

        AuroraTextField(
            value = serverUrl,
            onValueChange = { serverUrl = it },
            label = "Server URL",
            modifier = Modifier.fillMaxWidth(),
        )

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

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            AuroraButton(
                onClick = { vm.saveSettings(serverUrl, token, collection) },
                modifier = Modifier.weight(1f),
            ) {
                Text("Save")
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

        when (val c = connection) {
            is ConnectionState.Ok ->
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Icon(Icons.Default.CheckCircle, null, tint = MaterialTheme.colorScheme.primary)
                    Text("Connected", color = MaterialTheme.colorScheme.primary)
                }
            is ConnectionState.Failed ->
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Icon(Icons.Default.Error, null, tint = MaterialTheme.colorScheme.error)
                    Text(c.error, color = MaterialTheme.colorScheme.error)
                }
            else -> {}
        }
    }
}
