package com.axon.app.ui.nav

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Construction
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.FlightTakeoff
import androidx.compose.material.icons.rounded.HealthAndSafety
import androidx.compose.material.icons.rounded.MonitorHeart
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material.icons.rounded.Sync
import androidx.compose.material.icons.rounded.Terminal
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.AuroraStatusDot
import com.axon.app.ui.common.DotState
import com.axon.app.ui.common.Resource
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.management.ManagementViewModel
import com.axon.app.ui.setup.SetupViewModel
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

@Composable
internal fun ShellCommandReport(
    command: ShellCommand,
    setupVm: SetupViewModel = viewModel(),
    managementVm: ManagementViewModel = viewModel(),
) {
    val colors = AxonTheme.colors
    val smoke by setupVm.smokeState.collectAsStateWithLifecycle()
    val doctor by setupVm.doctorState.collectAsStateWithLifecycle()
    val stack by managementVm.statsState.collectAsStateWithLifecycle()
    val stackHealth by managementVm.doctorState.collectAsStateWithLifecycle()

    LaunchedEffect(command) {
        when (command) {
            ShellCommand.Preflight -> {
                setupVm.runSmoke()
                setupVm.runDoctor()
            }
            ShellCommand.Smoke -> setupVm.runSmoke()
            ShellCommand.Doctor -> setupVm.runDoctor()
            ShellCommand.Stack -> managementVm.runDoctor()
            ShellCommand.Monitor -> managementVm.loadStats()
            ShellCommand.Setup -> {
                setupVm.runSmoke()
                setupVm.runDoctor()
            }
            ShellCommand.Debug -> setupVm.runDoctor()
            else -> Unit
        }
    }

    val statusLabel = when (command) {
        ShellCommand.Preflight -> combineStatus(smoke, doctor)
        ShellCommand.Smoke -> resourceStatus(smoke)
        ShellCommand.Doctor -> resourceStatus(doctor)
        ShellCommand.Stack -> resourceStatus(stackHealth)
        ShellCommand.Monitor -> resourceStatus(stack)
        ShellCommand.Setup -> combineStatus(smoke, doctor)
        ShellCommand.Debug -> resourceStatus(doctor)
        ShellCommand.Dedupe, ShellCommand.Sync -> "READY"
    }
    val lines = commandLines(command, smoke, doctor, stack, stackHealth)
    val done = statusLabel == "PASSED" || statusLabel == "READY"
    val statusTone = when (statusLabel) {
        "ERROR" -> DotState.Failed
        "RUNNING" -> DotState.Running
        else -> DotState.Done
    }
    val statusText = when (statusLabel) {
        "RUNNING" -> "running"
        "ERROR" -> "error"
        else -> "done"
    }
    val summaryText = commandSummary(command, statusLabel, lines)

    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(
            modifier = Modifier
                .fillMaxWidth(0.92f)
                .widthIn(max = 430.dp)
                .padding(top = 18.dp),
            verticalArrangement = Arrangement.spacedBy(15.dp),
        ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(
                modifier = Modifier
                    .size(36.dp)
                    .clip(RoundedCornerShape(9.dp))
                    .background(colors.tint(colors.accentPrimary, 7, colors.pageBg))
                    .border(1.dp, colors.tint(colors.accentPrimary, 18, colors.pageBg), RoundedCornerShape(8.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(commandIcon(command), contentDescription = null, tint = colors.accentStrong.copy(alpha = 0.9f), modifier = Modifier.size(19.dp))
            }
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(command.title, color = colors.textPrimary, fontSize = 14.8.sp, fontWeight = FontWeight.ExtraBold, fontFamily = AxonTheme.fonts.display, maxLines = 1)
                Text("axon ${command.endpoint} · ${command.summary}", color = colors.textMuted.copy(alpha = 0.78f), fontSize = 10.8.sp, fontFamily = AxonTheme.fonts.mono, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                AuroraStatusDot(statusTone, size = 5.5.dp)
                Text(statusText, color = if (done) colors.success else if (statusLabel == "ERROR") colors.error else colors.accentStrong, fontSize = 10.7.sp, fontFamily = AxonTheme.fonts.mono)
            }
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(8.dp))
                .background(colors.control.copy(alpha = 0.035f))
                .border(1.dp, colors.borderDefault.copy(alpha = 0.08f), RoundedCornerShape(8.dp)),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(36.dp)
                    .border(1.dp, colors.borderDefault.copy(alpha = 0.08f))
                    .padding(horizontal = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                Icon(Icons.Rounded.Terminal, contentDescription = null, tint = colors.textMuted.copy(alpha = 0.72f), modifier = Modifier.size(15.dp))
                Text("OUTPUT", color = colors.textMuted.copy(alpha = 0.72f), fontSize = 10.2.sp, fontFamily = AxonTheme.fonts.mono, letterSpacing = 0.8.sp)
            }
            Column(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 13.dp, vertical = 13.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                lines.forEach { line ->
                    CommandOutputLine(line)
                }
            }
        }

        Row(
            modifier = Modifier
                .clip(RoundedCornerShape(999.dp))
                .background(colors.tint(colors.accentPrimary, 5, colors.pageBg))
                .border(1.dp, colors.tint(colors.accentPrimary, 18, colors.pageBg), RoundedCornerShape(999.dp))
                .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            AuroraStatusDot(if (statusLabel == "ERROR") DotState.Failed else DotState.Done, size = 5.5.dp)
        Text(summaryText, color = colors.accentStrong, fontSize = 11.sp, fontFamily = AxonTheme.fonts.mono, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
    }
}

@Composable
private fun CommandOutputLine(line: CommandLine) {
    val colors = AxonTheme.colors
    val color = lineToneColor(line.tone, colors)
    val text = humanizeJsonFragmentText(line.text)
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(9.dp)) {
        AuroraStatusDot(line.dot, size = 5.5.dp)
        Text(
            text,
            color = when (line.tone) {
                LineTone.Error, LineTone.Warn -> color
                LineTone.Ok -> colors.textPrimary
                LineTone.Muted -> colors.textMuted
            },
            fontSize = 11.2.sp,
            lineHeight = 15.2.sp,
            fontFamily = AxonTheme.fonts.mono,
            modifier = Modifier.weight(1f),
        )
    }
}

private data class CommandLine(val tone: LineTone, val text: String) {
    val dot: DotState = when (tone) {
        LineTone.Ok -> DotState.Done
        LineTone.Warn -> DotState.Warn
        LineTone.Error -> DotState.Failed
        LineTone.Muted -> DotState.Idle
    }
}

private enum class LineTone { Ok, Warn, Error, Muted }

private fun lineToneColor(tone: LineTone, colors: com.axon.app.ui.theme.AxonPalette): Color = when (tone) {
    LineTone.Ok -> colors.success
    LineTone.Warn -> colors.warn
    LineTone.Error -> colors.error
    LineTone.Muted -> colors.textMuted
}

private fun commandIcon(command: ShellCommand): ImageVector = when (command) {
    ShellCommand.Preflight -> Icons.Rounded.FlightTakeoff
    ShellCommand.Setup -> Icons.Rounded.Construction
    ShellCommand.Smoke -> Icons.Rounded.Wifi
    ShellCommand.Doctor -> Icons.Rounded.HealthAndSafety
    ShellCommand.Debug -> Icons.Rounded.Terminal
    ShellCommand.Dedupe -> Icons.Rounded.ContentCopy
    ShellCommand.Monitor -> Icons.Rounded.MonitorHeart
    ShellCommand.Sync -> Icons.Rounded.Sync
    ShellCommand.Stack -> Icons.Rounded.Storage
}

private fun commandLines(
    command: ShellCommand,
    smoke: Resource<String>,
    doctor: Resource<String>,
    stack: Resource<String>,
    stackHealth: Resource<String>,
): List<CommandLine> = when (command) {
    ShellCommand.Preflight -> resourceLines("healthz", smoke) + resourceLines("doctor", doctor)
    ShellCommand.Smoke -> resourceLines("healthz", smoke)
    ShellCommand.Doctor -> resourceLines("doctor", doctor)
    ShellCommand.Stack -> resourceLines("stack", stackHealth)
    ShellCommand.Monitor -> resourceLines("monitor", stack)
    ShellCommand.Dedupe -> listOf(CommandLine(LineTone.Warn, "dedupe command is not exposed by the Android API yet"))
    ShellCommand.Sync -> listOf(CommandLine(LineTone.Warn, "watch sync command is not exposed by the Android API yet"))
    ShellCommand.Setup -> listOf(
        CommandLine(LineTone.Muted, "~/.axon refreshed by server-side setup flow when invoked from CLI"),
        CommandLine(LineTone.Muted, ".env and config.toml values are loaded from the live panel API"),
    ) + resourceLines("healthz", smoke) + resourceLines("doctor", doctor)
    ShellCommand.Debug -> listOf(
        CommandLine(LineTone.Muted, "debug context uses the configured Android endpoint"),
        CommandLine(LineTone.Muted, ".env and config.toml are available from Config"),
    ) + resourceLines("doctor", doctor)
}

private fun resourceLines(label: String, resource: Resource<String>): List<CommandLine> = when (resource) {
    Resource.Idle -> listOf(CommandLine(LineTone.Muted, "$label · ready"))
    Resource.Loading -> listOf(CommandLine(LineTone.Muted, "$label · running"))
    is Resource.Error -> listOf(CommandLine(LineTone.Error, "$label · ${resource.message}"))
    is Resource.Ready -> splitCommandOutput(resource.value)
        .filter { it.isNotBlank() }
        .ifEmpty { listOf("ok") }
        .map { CommandLine(LineTone.Ok, it.removePrefix("$label · ").trim()) }
}

private fun splitCommandOutput(value: String): List<String> =
    if (value.contains('\n')) value.lines() else value.split(" · ")

private fun commandSummary(command: ShellCommand, statusLabel: String, lines: List<CommandLine>): String {
    val ok = lines.count { it.tone == LineTone.Ok }
    val warn = lines.count { it.tone == LineTone.Warn }
    val err = lines.count { it.tone == LineTone.Error }
    return when {
        statusLabel == "RUNNING" -> "running"
        err > 0 -> "$err error"
        warn > 0 -> "$ok ok · $warn warning"
        ok > 0 -> "$ok ok"
        command == ShellCommand.Setup -> "config ready"
        else -> "ready"
    }
}

private fun combineStatus(a: Resource<String>, b: Resource<String>): String = when {
    a is Resource.Error || b is Resource.Error -> "ERROR"
    a is Resource.Loading || b is Resource.Loading -> "RUNNING"
    a is Resource.Ready && b is Resource.Ready -> "PASSED"
    else -> "READY"
}

private fun resourceStatus(resource: Resource<String>): String = when (resource) {
    Resource.Idle -> "READY"
    Resource.Loading -> "RUNNING"
    is Resource.Ready -> "PASSED"
    is Resource.Error -> "ERROR"
}

private fun resourceLine(resource: Resource<String>): String = when (resource) {
    Resource.Idle -> "ready"
    Resource.Loading -> "running..."
    is Resource.Ready -> resource.value
    is Resource.Error -> resource.message
}
