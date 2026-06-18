package com.axon.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ArrowDropDown
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.data.auth.AuthMode
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant

@Composable
internal fun ConnectionTab(
    serverUrl: String,
    token: String,
    collection: String,
    onServerUrl: (String) -> Unit,
    onToken: (String) -> Unit,
    onCollection: (String) -> Unit,
    authMode: AuthMode,
    oauthStatus: OAuthUiStatus,
    onAuthMode: (AuthMode) -> Unit,
    onBeginOAuth: () -> Unit,
    onSignOutOAuth: () -> Unit,
    collections: CollectionListUiState,
    onRefreshCollections: () -> Unit,
    connection: TestConnectionState,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionLabel("Connection")
        CompactSettingField("Server", serverUrl, onServerUrl)
        if (serverUrl.startsWith("http://")) {
            AuroraCallout(
                message = "Cleartext HTTP is in use. Prefer HTTPS for non-Tailscale servers.",
                variant = AuroraCalloutVariant.Warn,
            )
        }
        AuthModeSelector(authMode = authMode, onAuthMode = onAuthMode)
        when (authMode) {
            AuthMode.Bearer -> CompactSettingField("Bearer token", token, onToken, visualTransformation = PasswordVisualTransformation())
            AuthMode.OAuth -> OAuthControls(
                status = oauthStatus,
                onBeginOAuth = onBeginOAuth,
                onSignOut = onSignOutOAuth,
            )
        }
        CollectionDropdownField(
            collection = collection,
            onCollection = onCollection,
            collections = collections,
            onRefreshCollections = onRefreshCollections,
        )
        when (val c = connection) {
            is TestConnectionState.Testing -> SettingsFeedbackBanner(
                message = "Testing connection...",
                kind = SettingsFeedbackKind.Info,
            )
            is TestConnectionState.Ok -> SettingsFeedbackBanner(
                message = c.warning?.let { "Connected. $it" } ?: "Connected.",
                kind = SettingsFeedbackKind.Success,
            )
            is TestConnectionState.Failed -> SettingsFeedbackBanner(
                message = humanizeJsonFragmentText(c.error),
                kind = SettingsFeedbackKind.Error,
            )
            else -> {}
        }
    }
}

@Composable
private fun AuthModeSelector(authMode: AuthMode, onAuthMode: (AuthMode) -> Unit) {
    val colors = AxonTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
        Text("Auth", color = colors.textMuted.copy(alpha = 0.8f), fontSize = 10.8.sp, fontFamily = AxonTheme.fonts.body)
        Row(horizontalArrangement = Arrangement.spacedBy(7.dp), modifier = Modifier.fillMaxWidth()) {
            AuthModeButton("Bearer", selected = authMode == AuthMode.Bearer, modifier = Modifier.weight(1f)) {
                onAuthMode(AuthMode.Bearer)
            }
            AuthModeButton("OAuth", selected = authMode == AuthMode.OAuth, modifier = Modifier.weight(1f)) {
                onAuthMode(AuthMode.OAuth)
            }
        }
    }
}

@Composable
private fun AuthModeButton(label: String, selected: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    Row(
        modifier = modifier
            .height(38.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(if (selected) colors.tint(colors.accentPrimary, 11, colors.pageBg) else colors.control.copy(alpha = 0.36f), RoundedCornerShape(8.dp))
            .border(1.dp, if (selected) colors.tint(colors.accentPrimary, 28, colors.pageBg) else colors.borderDefault.copy(alpha = 0.18f), RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            color = if (selected) colors.accentStrong else colors.textMuted,
            fontSize = 11.5.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun OAuthControls(
    status: OAuthUiStatus,
    onBeginOAuth: () -> Unit,
    onSignOut: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(7.dp)) {
        when (status) {
            OAuthUiStatus.SignedIn -> SettingsFeedbackBanner(
                message = "Signed in",
                kind = SettingsFeedbackKind.Success,
            )
            OAuthUiStatus.Starting -> SettingsFeedbackBanner(
                message = "Starting OAuth...",
                kind = SettingsFeedbackKind.Info,
            )
            OAuthUiStatus.Error -> SettingsFeedbackBanner(
                message = "OAuth sign-in failed",
                kind = SettingsFeedbackKind.Error,
            )
            OAuthUiStatus.Idle -> Unit
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            if (status == OAuthUiStatus.SignedIn) {
                CompactActionButton(
                    label = "Sign out",
                    onClick = onSignOut,
                    modifier = Modifier.weight(1f),
                    outlined = true,
                )
            } else {
                CompactActionButton(
                    label = if (status == OAuthUiStatus.Starting) "Starting OAuth..." else "Sign in",
                    onClick = onBeginOAuth,
                    modifier = Modifier.weight(1f),
                    enabled = status != OAuthUiStatus.Starting,
                )
            }
        }
        AuroraCallout(
            message = "Use com.axon.app://oauth2redirect for this internal build. Verified HTTPS App Links are preferred before broad production distribution.",
            variant = AuroraCalloutVariant.Info,
        )
    }
}

@Composable
private fun CollectionDropdownField(
    collection: String,
    onCollection: (String) -> Unit,
    collections: CollectionListUiState,
    onRefreshCollections: () -> Unit,
) {
    val colors = AxonTheme.colors
    var expanded by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Collection", color = colors.textMuted.copy(alpha = 0.8f), fontSize = 10.8.sp, fontFamily = AxonTheme.fonts.body)
            if (collections.loading) {
                Text("loading", color = colors.accentStrong, fontSize = 9.sp, fontFamily = AxonTheme.fonts.mono)
            }
            collections.error?.let {
                Text("bearer token required", color = colors.warn, fontSize = 9.sp, fontFamily = AxonTheme.fonts.mono)
            }
        }
        Box {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(40.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(colors.control.copy(alpha = 0.5f), RoundedCornerShape(8.dp))
                    .border(1.dp, colors.borderDefault.copy(alpha = 0.22f), RoundedCornerShape(8.dp))
                    .clickable {
                        expanded = true
                        if (collections.collections.isEmpty() && !collections.loading) onRefreshCollections()
                    }
                    .padding(horizontal = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    collection.ifBlank { "unset" },
                    color = if (collection.isBlank()) colors.textMuted else colors.textPrimary,
                    fontSize = 11.8.sp,
                    fontFamily = AxonTheme.fonts.mono,
                    modifier = Modifier.weight(1f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Icon(Icons.Rounded.ArrowDropDown, contentDescription = null, tint = colors.textMuted, modifier = Modifier.size(18.dp))
            }
            DropdownMenu(
                expanded = expanded,
                onDismissRequest = { expanded = false },
                modifier = Modifier.background(colors.panelStrong),
            ) {
                val options = (collections.collections + collection)
                    .filter { it.isNotBlank() }
                    .distinct()
                    .sorted()
                if (options.isEmpty()) {
                    DropdownMenuItem(
                        text = { Text("No collections loaded", color = colors.textMuted, fontFamily = AxonTheme.fonts.body) },
                        onClick = {
                            expanded = false
                            onRefreshCollections()
                        },
                    )
                } else {
                    options.forEach { option ->
                        DropdownMenuItem(
                            text = { Text(option, color = colors.textPrimary, fontFamily = AxonTheme.fonts.mono, fontSize = 12.sp) },
                            onClick = {
                                onCollection(option)
                                expanded = false
                            },
                        )
                    }
                }
            }
        }
    }
}
