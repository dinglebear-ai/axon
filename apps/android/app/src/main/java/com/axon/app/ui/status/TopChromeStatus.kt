package com.axon.app.ui.status

import android.content.ClipData
import android.content.ClipboardManager
import android.widget.Toast
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.ErrorOutline
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Sync
import androidx.compose.material3.Surface
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.AxonElevation
import com.axon.app.ui.common.CompactActionButton
import com.axon.app.ui.common.axonElevation
import com.axon.app.ui.theme.AxonTheme
import java.net.URI
import java.text.DateFormat
import java.util.Date

data class StatusDiagnostics(
    val serverUrl: String = "",
    val authMode: String = "",
    val collection: String = "",
)

@Composable
fun TopChromeStatus(
    modifier: Modifier = Modifier,
    vm: ConnectionStatusViewModel = viewModel(),
    onOfflineClick: (() -> Unit)? = null,
    diagnostics: StatusDiagnostics = StatusDiagnostics(),
) {
    val colors = AxonTheme.colors
    val state by vm.state.collectAsStateWithLifecycle()
    val latencyMs by vm.latencyMs.collectAsStateWithLifecycle()
    var detailOpen by remember { mutableStateOf(false) }
    var lastCheckAt by remember { mutableStateOf<Long?>(null) }
    LaunchedEffect(state, latencyMs) {
        if (state != ConnectionState.Checking || latencyMs != null) {
            lastCheckAt = System.currentTimeMillis()
        }
    }
    val dot = when (state) {
        ConnectionState.Checking -> colors.accentStrong
        ConnectionState.Online -> colors.success
        ConnectionState.Offline -> colors.textMuted
    }
    val label = when (state) {
        ConnectionState.Checking -> "Checking"
        ConnectionState.Online -> latencyMs?.let { "${it.coerceAtMost(999)}ms" } ?: "Online"
        ConnectionState.Offline -> "Offline"
    }
    val shape = RoundedCornerShape(999.dp)

    Row(
        modifier = modifier
            .height(30.dp)
            .background(colors.control.copy(alpha = 0.42f), shape)
            .border(1.dp, dot.copy(alpha = 0.34f), shape)
            .clickable { detailOpen = true }
            .clearAndSetSemantics {
                contentDescription = "Axon status, $label"
                role = Role.Button
                onClick("Open Axon status") {
                    detailOpen = true
                    true
                }
            }
            .padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            modifier = Modifier
                .size(6.dp)
                .clip(shape)
                .background(dot.copy(alpha = 0.92f)),
        )
        Text(
            label,
            color = colors.textMuted.copy(alpha = if (state == ConnectionState.Offline) 0.94f else 0.86f),
            fontSize = 11.2.sp,
            lineHeight = 14.sp,
            fontFamily = AxonTheme.fonts.body,
        )
    }
    if (detailOpen) {
        StatusDetailDialog(
            state = state,
            latencyMs = latencyMs,
            lastCheckAt = lastCheckAt,
            serverUrl = diagnostics.serverUrl,
            authMode = diagnostics.authMode,
            collection = diagnostics.collection,
            onDismiss = { detailOpen = false },
            onRetry = vm::refresh,
            onOpenSettings = onOfflineClick,
        )
    }
}

@Composable
private fun StatusDetailDialog(
    state: ConnectionState,
    latencyMs: Long?,
    lastCheckAt: Long?,
    serverUrl: String,
    authMode: String,
    collection: String,
    onDismiss: () -> Unit,
    onRetry: () -> Unit,
    onOpenSettings: (() -> Unit)?,
) {
    val context = LocalContext.current
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(14.dp)
    val lastCheck = lastCheckAt?.let {
        DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(it))
    } ?: "Not checked yet"
    val statusLabel = when (state) {
        ConnectionState.Checking -> "Checking"
        ConnectionState.Online -> "Online"
        ConnectionState.Offline -> "Offline"
    }
    val safeServerUrl = remember(serverUrl) { redactUrlUserInfo(serverUrl) }
    val curlCommand = remember(safeServerUrl) {
        val base = safeServerUrl.trim().trimEnd('/')
        if (base.isBlank()) "curl -i http://<axon-host>/healthz" else "curl -i $base/healthz"
    }
    fun copyDiagnostics() {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        if (clipboard == null) {
            Toast.makeText(context, "Clipboard is unavailable", Toast.LENGTH_LONG).show()
            return
        }
        runCatching {
            clipboard.setPrimaryClip(ClipData.newPlainText("Axon health check", curlCommand))
        }.fold(
            onSuccess = { Toast.makeText(context, "Health check copied", Toast.LENGTH_SHORT).show() },
            onFailure = { Toast.makeText(context, "Could not copy health check", Toast.LENGTH_LONG).show() },
        )
    }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .widthIn(max = 360.dp)
                .axonElevation(shape, AxonElevation.Floating),
            shape = shape,
            color = colors.panelStrong,
            border = BorderStroke(1.dp, colors.borderStrong.copy(alpha = 0.48f)),
        ) {
            Column(
                modifier = Modifier
                    .heightIn(max = 560.dp)
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    "Axon status",
                    color = colors.textPrimary,
                    fontSize = 17.sp,
                    lineHeight = 22.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = AxonTheme.fonts.body,
                )
                if (state == ConnectionState.Offline) {
                    OfflineRecoveryPanel(
                        serverUrl = serverUrl,
                        authMode = authMode,
                        curlCommand = curlCommand,
                        onRetry = onRetry,
                        onOpenSettings = onOpenSettings?.let {
                            {
                                onDismiss()
                                it()
                            }
                        },
                        onCopyDiagnostics = ::copyDiagnostics,
                    )
                }
                StatusDetailRow("Status", statusLabel)
                StatusDetailRow("Server", safeServerUrl)
                StatusDetailRow("Auth", authMode)
                StatusDetailRow("Collection", collection.ifBlank { "unset" })
                StatusDetailRow("Last check", lastCheck)
                StatusDetailRow("Latency", latencyMs?.let { "${it.coerceAtMost(999)}ms" } ?: "n/a")
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
                    if (state == ConnectionState.Offline) {
                        StatusDialogAction(
                            label = "Done",
                            onClick = onDismiss,
                            modifier = Modifier.weight(1f),
                            outlined = true,
                        )
                    } else {
                        StatusDialogAction(
                            label = if (state == ConnectionState.Checking) "Checking..." else "Retry",
                            onClick = onRetry,
                            modifier = Modifier.weight(1f),
                            enabled = state != ConnectionState.Checking,
                        )
                        StatusDialogAction(
                            label = "Done",
                            onClick = onDismiss,
                            modifier = Modifier.weight(1f),
                            outlined = true,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun OfflineRecoveryPanel(
    serverUrl: String,
    authMode: String,
    curlCommand: String,
    onRetry: () -> Unit,
    onOpenSettings: (() -> Unit)?,
    onCopyDiagnostics: () -> Unit,
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(10.dp)
    val serverHint = if (serverUrl.isBlank()) {
        "Add your Axon server URL in Settings, then test the connection."
    } else {
        "Verify the server is reachable, then check auth and collection settings."
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(colors.error.copy(alpha = 0.08f), shape)
            .border(1.dp, colors.error.copy(alpha = 0.24f), shape)
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(9.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Rounded.ErrorOutline, contentDescription = null, tint = colors.error, modifier = Modifier.size(18.dp))
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    "Connection needs attention",
                    color = colors.textPrimary,
                    fontSize = 13.4.sp,
                    lineHeight = 17.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = AxonTheme.fonts.body,
                )
                Text(
                    "$serverHint Auth mode: $authMode.",
                    color = colors.textMuted,
                    fontSize = 11.6.sp,
                    lineHeight = 16.sp,
                    fontFamily = AxonTheme.fonts.body,
                )
            }
        }
        StatusDiagnosticCommand(curlCommand)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            StatusDialogAction(
                label = "Retry",
                onClick = onRetry,
                modifier = Modifier.weight(1f),
                icon = Icons.Rounded.Sync,
            )
            if (onOpenSettings != null) {
                StatusDialogAction(
                    label = "Settings",
                    onClick = onOpenSettings,
                    modifier = Modifier.weight(1f),
                    outlined = true,
                    icon = Icons.Rounded.Settings,
                )
            }
            StatusDialogAction(
                label = "Copy",
                onClick = onCopyDiagnostics,
                modifier = Modifier.weight(1f),
                outlined = true,
                icon = Icons.Rounded.ContentCopy,
            )
        }
    }
}

@Composable
private fun StatusDiagnosticCommand(command: String) {
    val colors = AxonTheme.colors
    Text(
        command,
        color = colors.textMuted.copy(alpha = 0.92f),
        fontSize = 11.sp,
        lineHeight = 15.sp,
        fontFamily = AxonTheme.fonts.mono,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(colors.pageBg.copy(alpha = 0.58f), RoundedCornerShape(8.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.16f), RoundedCornerShape(8.dp))
            .padding(horizontal = 10.dp, vertical = 8.dp),
    )
}

internal fun redactUrlUserInfo(value: String): String {
    val trimmed = value.trim()
    if (trimmed.isBlank()) return trimmed
    return runCatching {
        val uri = URI(trimmed)
        fun redactedUri(): String = URI(
            uri.scheme,
            null,
            uri.host,
            uri.port,
            uri.path,
            null,
            null,
        ).toString()
        if (uri.rawUserInfo == null) {
            if (uri.rawQuery == null && uri.rawFragment == null) trimmed else redactedUri()
        } else if (uri.host == null) {
            trimmed.replace(Regex("(?<=://)[^/@]+@"), "").substringBefore('?').substringBefore('#')
        } else {
            redactedUri()
        }
    }.getOrElse { trimmed.replace(Regex("(?<=://)[^/@]+@"), "").substringBefore('?').substringBefore('#') }
}

@Composable
private fun StatusDetailRow(label: String, value: String) {
    val colors = AxonTheme.colors
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxWidth()) {
        Text(
            label,
            color = colors.textMuted,
            fontSize = 11.4.sp,
            lineHeight = 15.sp,
            fontFamily = AxonTheme.fonts.body,
            modifier = Modifier.weight(0.38f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            value,
            color = colors.textPrimary,
            fontSize = 12.sp,
            lineHeight = 16.sp,
            fontFamily = AxonTheme.fonts.mono,
            modifier = Modifier.weight(0.62f),
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun StatusDialogAction(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    outlined: Boolean = false,
    icon: ImageVector? = null,
) {
    CompactActionButton(
        label = label,
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        outlined = outlined,
        icon = icon,
        heightDp = 42,
    )
}
