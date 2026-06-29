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
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.ErrorOutline
import androidx.compose.material.icons.rounded.RadioButtonUnchecked
import androidx.compose.material.icons.rounded.Route
import androidx.compose.material.icons.rounded.Sync
import androidx.compose.material.icons.rounded.VerifiedUser
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
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.data.auth.AuthMode
import com.axon.app.ui.common.AxonElevation
import com.axon.app.ui.common.CompactActionButton
import com.axon.app.ui.common.RecoveryActionCard
import com.axon.app.ui.common.axonElevation
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
    onTestConnection: () -> Unit,
    connection: TestConnectionState,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(16.dp)) {
        SectionLabel("Connection")
        ConnectionSetupSummary(
            serverUrl = serverUrl,
            token = token,
            collection = collection,
            authMode = authMode,
            oauthStatus = oauthStatus,
            connection = connection,
            onTestConnection = onTestConnection,
        )
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
        if (connection is TestConnectionState.Failed) {
            ConnectionRecoveryChecklist(
                authMode = authMode,
                onRetry = onTestConnection,
            )
        }
    }
}

@Composable
private fun ConnectionSetupSummary(
    serverUrl: String,
    token: String,
    collection: String,
    authMode: AuthMode,
    oauthStatus: OAuthUiStatus,
    connection: TestConnectionState,
    onTestConnection: () -> Unit,
) {
    val colors = AxonTheme.colors
    val serverReady = serverUrl.isNotBlank()
    val authReady = when (authMode) {
        AuthMode.Bearer -> token.isNotBlank()
        AuthMode.OAuth -> oauthStatus == OAuthUiStatus.SignedIn
    }
    val collectionReady = collection.isNotBlank()
    val connected = connection is TestConnectionState.Ok
    val actionLabel = when {
        connection is TestConnectionState.Testing -> "Testing..."
        !serverReady -> "Add server URL"
        !authReady && authMode == AuthMode.Bearer -> "Add token below"
        !authReady && authMode == AuthMode.OAuth -> "Sign in with OAuth"
        else -> "Test connection"
    }
    val summary = when {
        connected -> "Server health endpoint is reachable. Auth and collection are set locally."
        connection is TestConnectionState.Failed -> "Connection test failed. Check the details below and retry."
        !serverReady -> "Enter your Axon server URL, then sign in with OAuth."
        !authReady && authMode == AuthMode.Bearer -> "Bearer mode needs a token before testing most Axon servers."
        !authReady && authMode == AuthMode.OAuth -> "OAuth is the recommended sign-in flow. No bearer token is needed."
        !collectionReady -> "Pick the Qdrant collection used for requests."
        else -> "Test the saved endpoint before leaving setup."
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .axonElevation(RoundedCornerShape(12.dp), AxonElevation.Card)
            .clip(RoundedCornerShape(12.dp))
            .background(colors.panelStrong.copy(alpha = 0.72f), RoundedCornerShape(12.dp))
            .border(1.dp, colors.borderStrong.copy(alpha = 0.36f), RoundedCornerShape(12.dp))
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            SetupStatusIcon(connected = connected, inProgress = connection is TestConnectionState.Testing, failed = connection is TestConnectionState.Failed)
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    "Setup summary",
                    color = colors.textPrimary,
                    fontSize = 15.sp,
                    lineHeight = 19.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = AxonTheme.fonts.body,
                )
                Text(
                    summary,
                    color = colors.textMuted,
                    fontSize = 12.sp,
                    lineHeight = 16.sp,
                    fontFamily = AxonTheme.fonts.body,
                )
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            SetupStep("Server", serverReady, modifier = Modifier.weight(1f))
            SetupStep("Auth", authReady, modifier = Modifier.weight(1f))
            SetupStep("Collection", collectionReady, modifier = Modifier.weight(1f))
        }
        CompactActionButton(
            label = actionLabel,
            onClick = onTestConnection,
            enabled = serverReady && connection !is TestConnectionState.Testing && authReady,
            modifier = Modifier.fillMaxWidth(),
            outlined = connected,
            icon = Icons.Rounded.Sync,
        )
    }
}

@Composable
private fun ConnectionRecoveryChecklist(
    authMode: AuthMode,
    onRetry: () -> Unit,
) {
    val authHint = when (authMode) {
        AuthMode.Bearer -> "Confirm the bearer token is current and includes Axon access."
        AuthMode.OAuth -> "Refresh OAuth sign-in below if the server rejects the token."
    }
    RecoveryActionCard(
        title = "Connection recovery",
        message = "Check server reachability, auth, and collection spelling. $authHint",
        primaryLabel = "Retry test",
        onPrimary = onRetry,
        icon = Icons.Rounded.Route,
    )
}

@Composable
private fun SetupStatusIcon(connected: Boolean, inProgress: Boolean, failed: Boolean) {
    val colors = AxonTheme.colors
    val icon = when {
        connected -> Icons.Rounded.CheckCircle
        failed -> Icons.Rounded.ErrorOutline
        inProgress -> Icons.Rounded.Sync
        else -> Icons.Rounded.RadioButtonUnchecked
    }
    val tint = when {
        connected -> colors.success
        failed -> colors.error
        inProgress -> colors.accentStrong
        else -> colors.textMuted
    }
    Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(22.dp))
}

@Composable
private fun SetupStep(label: String, ready: Boolean, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    Row(
        modifier = modifier
            .height(34.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(if (ready) colors.tint(colors.success, 8, colors.panelStrong) else colors.control.copy(alpha = 0.34f), RoundedCornerShape(8.dp))
            .border(1.dp, if (ready) colors.success.copy(alpha = 0.28f) else colors.borderDefault.copy(alpha = 0.18f), RoundedCornerShape(8.dp))
            .padding(horizontal = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(5.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (ready) Icons.Rounded.CheckCircle else Icons.Rounded.RadioButtonUnchecked,
            contentDescription = null,
            tint = if (ready) colors.success else colors.textMuted,
            modifier = Modifier.size(14.dp),
        )
        Text(
            label,
            color = if (ready) colors.textPrimary else colors.textMuted,
            fontSize = 10.6.sp,
            lineHeight = 13.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun AuthModeSelector(authMode: AuthMode, onAuthMode: (AuthMode) -> Unit) {
    val colors = AxonTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(7.dp)) {
        Text("Auth", color = colors.textMuted.copy(alpha = 0.86f), fontSize = 13.sp, lineHeight = 17.sp, fontFamily = AxonTheme.fonts.body)
        Row(horizontalArrangement = Arrangement.spacedBy(9.dp), modifier = Modifier.fillMaxWidth()) {
            AuthModeButton(
                label = "OAuth",
                detail = "Recommended",
                selected = authMode == AuthMode.OAuth,
                modifier = Modifier.weight(1f),
                icon = Icons.Rounded.VerifiedUser,
            ) {
                onAuthMode(AuthMode.OAuth)
            }
            AuthModeButton(
                label = "Bearer",
                detail = "Fallback",
                selected = authMode == AuthMode.Bearer,
                modifier = Modifier.weight(1f),
            ) {
                onAuthMode(AuthMode.Bearer)
            }
        }
    }
}

@Composable
private fun AuthModeButton(
    label: String,
    detail: String,
    selected: Boolean,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    onClick: () -> Unit,
) {
    val colors = AxonTheme.colors
    Column(
        modifier = modifier
            .height(54.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(if (selected) colors.tint(colors.accentPrimary, 11, colors.pageBg) else colors.control.copy(alpha = 0.36f), RoundedCornerShape(8.dp))
            .border(1.dp, if (selected) colors.tint(colors.accentPrimary, 28, colors.pageBg) else colors.borderDefault.copy(alpha = 0.18f), RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
            icon?.let { Icon(it, contentDescription = null, tint = if (selected) colors.accentStrong else colors.textMuted, modifier = Modifier.size(14.dp)) }
            Text(
                label,
                color = if (selected) colors.accentStrong else colors.textMuted,
                fontSize = 13.6.sp,
                lineHeight = 18.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Text(
            detail,
            color = if (selected) colors.accentStrong.copy(alpha = 0.78f) else colors.textMuted.copy(alpha = 0.78f),
            fontSize = 10.4.sp,
            lineHeight = 13.sp,
            fontWeight = FontWeight.Medium,
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
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        when (status) {
            OAuthUiStatus.SignedIn -> SettingsFeedbackBanner(
                message = "OAuth signed in. This server URL is saved for future launches.",
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
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            if (status == OAuthUiStatus.SignedIn) {
                CompactActionButton(
                    label = "Sign out",
                    onClick = onSignOut,
                    modifier = Modifier.weight(1f),
                    outlined = true,
                )
            } else {
                CompactActionButton(
                    label = if (status == OAuthUiStatus.Starting) "Starting OAuth..." else "Sign in with OAuth",
                    onClick = onBeginOAuth,
                    modifier = Modifier.weight(1f),
                    enabled = status != OAuthUiStatus.Starting,
                )
            }
        }
        SettingsFeedbackBanner(
            message = "OAuth opens your browser and returns here after approval. Bearer tokens remain available as a fallback.",
            kind = SettingsFeedbackKind.Info,
            modifier = Modifier.fillMaxWidth(),
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
    Column(verticalArrangement = Arrangement.spacedBy(7.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(9.dp)) {
            Text("Collection", color = colors.textMuted.copy(alpha = 0.86f), fontSize = 13.sp, lineHeight = 17.sp, fontFamily = AxonTheme.fonts.body)
            if (collections.loading) {
                Text("loading", color = colors.accentStrong, fontSize = 10.sp, fontFamily = AxonTheme.fonts.mono)
            }
            collections.error?.let {
                Text("could not load", color = colors.warn, fontSize = 10.sp, fontFamily = AxonTheme.fonts.mono)
            }
        }
        Box {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(56.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(colors.control.copy(alpha = 0.5f), RoundedCornerShape(8.dp))
                    .border(1.dp, colors.borderDefault.copy(alpha = 0.22f), RoundedCornerShape(8.dp))
                    .clickable {
                        expanded = true
                        if (collections.collections.isEmpty() && !collections.loading) onRefreshCollections()
                    }
                    .padding(horizontal = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(
                    collection.ifBlank { "unset" },
                    color = if (collection.isBlank()) colors.textMuted else colors.textPrimary,
                    fontSize = 14.sp,
                    lineHeight = 19.sp,
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
