package com.axon.app.ui.nav

import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Construction
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.FlightTakeoff
import androidx.compose.material.icons.rounded.HealthAndSafety
import androidx.compose.material.icons.rounded.History
import androidx.compose.material.icons.rounded.Home
import androidx.compose.material.icons.rounded.Hub
import androidx.compose.material.icons.rounded.Menu
import androidx.compose.material.icons.rounded.MonitorHeart
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material.icons.rounded.Sync
import androidx.compose.material.icons.rounded.TaskAlt
import androidx.compose.material.icons.rounded.Terminal
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavController
import com.axon.app.ui.ask.AskScreen
import com.axon.app.ui.common.AuroraStatusDot
import com.axon.app.ui.common.DotState
import com.axon.app.ui.common.Resource
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.jobs.JobsScreen
import com.axon.app.ui.knowledge.KnowledgeScreen
import com.axon.app.ui.knowledge.KnowledgeTab
import com.axon.app.ui.management.ManagementDrawerContent
import com.axon.app.ui.management.ManagementViewModel
import com.axon.app.ui.sessions.SessionsDrawerContent
import com.axon.app.ui.settings.SettingsScreen
import com.axon.app.ui.status.TopChromeStatus
import com.axon.app.ui.setup.SetupDrawerContent
import com.axon.app.ui.setup.SetupViewModel
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

sealed interface ShellOverlay {
    val title: String

    data class Knowledge(val tab: KnowledgeTab) : ShellOverlay {
        override val title: String = tab.title
    }

    data object Settings : ShellOverlay {
        override val title: String = "Config"
    }

    data class Command(val command: ShellCommand) : ShellOverlay {
        override val title: String = command.title
    }
}

enum class ShellCommand(val title: String, val endpoint: String, val summary: String) {
    Dedupe("Dedupe", "dedupe", "merge duplicate vectors"),
    Monitor("Monitor", "monitor", "live job + resource monitor"),
    Sync("Sync", "sync", "sitemap backfill + re-embed"),
    Stack("Stack", "stack", "compose service status"),
    Preflight("Preflight", "preflight", "prerequisites + readiness"),
    Setup("Setup", "setup", "init + compose up + preflight"),
    Smoke("Smoke", "smoke", "TEI prewarm + crawl/ask proof"),
    Doctor("Doctor", "doctor", "service health"),
    Debug("Debug", "debug", "env + paths + versions"),
}

private data class SidebarItem(
    val label: String,
    val value: String,
    val icon: androidx.compose.ui.graphics.vector.ImageVector,
)

@Composable
fun RailScaffold(navController: NavController, modifier: Modifier = Modifier) {
    var activePage by remember { mutableStateOf<DrawerSection?>(null) }
    var activeOverlay by remember { mutableStateOf<ShellOverlay?>(null) }
    var sidebarOpen by remember { mutableStateOf(false) }
    val colors = AxonTheme.colors

    val sidebarItems = remember {
        listOf(
            SidebarItem("Ask", "ask", Icons.Rounded.Home),
            SidebarItem("Sessions", "sessions", Icons.Rounded.History),
            SidebarItem("Jobs", "jobs", Icons.Rounded.TaskAlt),
            SidebarItem("Knowledge", "knowledge", Icons.Rounded.Hub),
            SidebarItem("Management", "management", Icons.Rounded.Tune),
            SidebarItem("Setup", "setup", Icons.Rounded.Construction),
        )
    }
    fun selectedValue(): String = when (activePage) {
        null -> "ask"
        DrawerSection.Sessions -> "sessions"
        DrawerSection.Jobs -> "jobs"
        DrawerSection.Knowledge -> "knowledge"
        DrawerSection.Management -> "management"
        DrawerSection.Setup -> "setup"
    }
    fun selectSidebarValue(value: String) {
        activeOverlay = null
        activePage = when (value) {
            "sessions" -> DrawerSection.Sessions
            "jobs" -> DrawerSection.Jobs
            "knowledge" -> DrawerSection.Knowledge
            "management" -> DrawerSection.Management
            "setup" -> DrawerSection.Setup
            else -> null
        }
        sidebarOpen = false
    }
    fun openOverlay(overlay: ShellOverlay) {
        activeOverlay = overlay
        sidebarOpen = false
    }

    BackHandler(enabled = activeOverlay != null || sidebarOpen || activePage != null) {
        if (activeOverlay != null) {
            activeOverlay = null
        } else if (sidebarOpen) {
            sidebarOpen = false
        } else {
            activePage = null
        }
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(colors.pageBg)
    ) {
        Column(modifier = Modifier.fillMaxSize().statusBarsPadding()) {
            AxonTopBar(
                title = activeOverlay?.title ?: activePage?.title() ?: "Ask",
                sidebarOpen = sidebarOpen,
                onToggleSidebar = { sidebarOpen = !sidebarOpen },
            )
            Box(Modifier.fillMaxWidth().height(1.dp).background(colors.borderDefault.copy(alpha = 0.32f)))
            Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
                Box(modifier = Modifier.fillMaxSize().clip(RoundedCornerShape(0.dp))) {
                    val overlay = activeOverlay
                    if (overlay == null) {
                        ShellPageContent(
                            page = activePage,
                            navController = navController,
                            onOpenOverlay = ::openOverlay,
                        )
                    } else {
                        ShellOverlayContent(
                            overlay = overlay,
                            navController = navController,
                            onBack = { activeOverlay = null },
                            onHome = {
                                activeOverlay = null
                                activePage = null
                            },
                        )
                    }
                }
                if (sidebarOpen) {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .background(Color(0xFF040A0E).copy(alpha = 0.50f))
                            .clickable(remember { MutableInteractionSource() }, indication = null) {
                                sidebarOpen = false
                            },
                    )
                    AxonSidebarSheet(
                        items = sidebarItems,
                        selected = selectedValue(),
                        onSelect = ::selectSidebarValue,
                    )
                }
            }
        }
    }
}

@Composable
private fun AxonSidebarSheet(
    items: List<SidebarItem>,
    selected: String,
    onSelect: (String) -> Unit,
) {
    val colors = AxonTheme.colors
    Column(
        modifier = Modifier
            .width(196.dp)
            .fillMaxHeight()
            .background(colors.panelStrong)
            .border(width = 1.dp, color = colors.borderDefault)
            .padding(horizontal = 10.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(38.dp)
                .padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            AxonMarkGlyph(Modifier.size(24.dp))
            Text(
                "Axon",
                color = colors.textPrimary,
                fontSize = 15.sp,
                fontWeight = FontWeight.ExtraBold,
                fontFamily = AxonTheme.fonts.display,
            )
        }
        Spacer(Modifier.height(2.dp))
        items.forEach { item ->
            AxonSidebarRow(
                item = item,
                selected = item.value == selected,
                onClick = { onSelect(item.value) },
            )
        }
    }
}

@Composable
private fun AxonSidebarRow(
    item: SidebarItem,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(13.dp)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(46.dp)
            .clip(shape)
            .background(if (selected) colors.tint(colors.accentPrimary, 11, colors.panelStrong) else colors.control.copy(alpha = 0.32f), shape)
            .border(1.dp, if (selected) colors.tint(colors.accentPrimary, 28, colors.panelStrong) else colors.borderDefault.copy(alpha = 0.55f), shape)
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick)
            .padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(11.dp),
    ) {
        Box(
            modifier = Modifier
                .width(3.dp)
                .height(22.dp)
                .clip(RoundedCornerShape(999.dp))
                .background(if (selected) colors.accentPrimary else colors.borderDefault.copy(alpha = 0.0f)),
        )
        Icon(
            imageVector = item.icon,
            contentDescription = item.label,
            tint = if (selected) colors.accentStrong else colors.textMuted,
            modifier = Modifier.size(18.dp),
        )
        Text(
            text = item.label,
            color = if (selected) colors.textPrimary else colors.textMuted,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun ShellPageContent(
    page: DrawerSection?,
    navController: NavController,
    onOpenOverlay: (ShellOverlay) -> Unit,
) {
    when (page) {
        null -> AskScreen(onOpenDocument = { url -> navController.navigate(DocumentRoute(Uri.encode(url))) })
        DrawerSection.Sessions -> PageSurface { SessionsDrawerContent() }
        DrawerSection.Jobs -> JobsScreen()
        DrawerSection.Knowledge -> KnowledgeScreen(
            onOpenTab = { tab -> onOpenOverlay(ShellOverlay.Knowledge(tab)) },
            onOpenDocument = { url -> navController.navigate(DocumentRoute(Uri.encode(url))) },
        )
        DrawerSection.Management -> PageSurface {
            ManagementDrawerContent(
                onOpenDedupe = { onOpenOverlay(ShellOverlay.Command(ShellCommand.Dedupe)) },
                onOpenMonitor = { onOpenOverlay(ShellOverlay.Command(ShellCommand.Monitor)) },
                onOpenSync = { onOpenOverlay(ShellOverlay.Command(ShellCommand.Sync)) },
                onOpenStack = { onOpenOverlay(ShellOverlay.Command(ShellCommand.Stack)) },
                onOpenSettings = { onOpenOverlay(ShellOverlay.Settings) },
            )
        }
        DrawerSection.Setup -> PageSurface {
            SetupDrawerContent(
                onOpenPreflight = { onOpenOverlay(ShellOverlay.Command(ShellCommand.Preflight)) },
                onOpenSetup = { onOpenOverlay(ShellOverlay.Command(ShellCommand.Setup)) },
                onOpenSmoke = { onOpenOverlay(ShellOverlay.Command(ShellCommand.Smoke)) },
                onOpenDoctor = { onOpenOverlay(ShellOverlay.Command(ShellCommand.Doctor)) },
                onOpenDebug = { onOpenOverlay(ShellOverlay.Command(ShellCommand.Debug)) },
            )
        }
    }
}

@Composable
private fun PageSurface(content: @Composable () -> Unit) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(AxonTheme.colors.pageBg)
            .padding(10.dp),
    ) {
        content()
    }
}

private fun DrawerSection.title(): String = when (this) {
    DrawerSection.Sessions -> "Sessions"
    DrawerSection.Jobs -> "Jobs"
    DrawerSection.Knowledge -> "Knowledge"
    DrawerSection.Management -> "Management"
    DrawerSection.Setup -> "Setup"
}

@Composable
private fun ShellOverlayContent(
    overlay: ShellOverlay,
    navController: NavController,
    onBack: () -> Unit,
    onHome: () -> Unit,
) {
    val colors = AxonTheme.colors
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(colors.pageBg),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(40.dp)
                .background(colors.panelMedium)
                .border(1.dp, colors.borderDefault)
                .padding(horizontal = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(
                Icons.AutoMirrored.Rounded.ArrowBack,
                contentDescription = "Back",
                tint = colors.textMuted.copy(alpha = 0.86f),
                modifier = Modifier
                    .size(25.dp)
                    .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onBack)
                    .padding(5.dp),
            )
            Text(
                overlay.title,
                color = colors.textPrimary,
                fontSize = 14.sp,
                fontWeight = FontWeight.ExtraBold,
                fontFamily = AxonTheme.fonts.display,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Icon(
                Icons.Rounded.Close,
                contentDescription = "Close",
                tint = colors.textMuted,
                modifier = Modifier
                    .size(25.dp)
                    .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onHome)
                    .padding(5.dp),
            )
        }
        Box(modifier = Modifier.fillMaxSize()) {
            when (overlay) {
                is ShellOverlay.Knowledge -> KnowledgeScreen(
                    initialTab = overlay.tab,
                    showChrome = false,
                    onOpenDocument = { url -> navController.navigate(DocumentRoute(Uri.encode(url))) },
                )
                ShellOverlay.Settings -> SettingsScreen()
                is ShellOverlay.Command -> ShellCommandReport(command = overlay.command)
            }
        }
    }
}


@Composable
private fun AxonTopBar(
    title: String,
    sidebarOpen: Boolean,
    onToggleSidebar: () -> Unit,
) {
    val colors = AxonTheme.colors
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(42.dp)
            .background(colors.navBg)
            .padding(start = 13.dp, end = 11.dp),
    ) {
        Row(
            modifier = Modifier.align(Alignment.CenterStart),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                Icons.Rounded.Menu,
                contentDescription = if (sidebarOpen) "Collapse sidebar" else "Open sidebar",
                tint = colors.textMuted,
                modifier = Modifier
                    .size(25.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onToggleSidebar)
                    .padding(5.dp),
            )
            Spacer(Modifier.width(10.dp))
            Text("Axon", color = colors.textPrimary.copy(alpha = 0.88f), fontSize = 13.2.sp, fontWeight = FontWeight.ExtraBold, fontFamily = AxonTheme.fonts.display)
            Spacer(Modifier.width(8.dp))
            AuroraStatusDot(DotState.Done, size = 5.dp)
        }
        Text(
            title,
            color = colors.textPrimary.copy(alpha = 0.90f),
            fontSize = 13.2.sp,
            fontWeight = FontWeight.ExtraBold,
            fontFamily = AxonTheme.fonts.display,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .align(Alignment.Center)
                .widthIn(max = 180.dp),
        )
        TopChromeStatus(modifier = Modifier.align(Alignment.CenterEnd))
    }
}

@Composable
fun AxonMarkGlyph(modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    Canvas(modifier = modifier) {
        val cx = size.width / 2f
        val nodeRadius = size.minDimension * 0.095f
        val stroke = size.minDimension * 0.055f
        val ys = listOf(
            size.height * 0.26f,
            size.height * 0.42f,
            size.height * 0.58f,
            size.height * 0.74f,
        )
        drawLine(colors.borderStrong, Offset(cx, ys[0] + nodeRadius), Offset(cx, ys[3] - nodeRadius), stroke, StrokeCap.Round)
        drawLine(colors.borderStrong, Offset(cx, ys[0] - nodeRadius * 1.4f), Offset(cx - size.width * 0.24f, 0f), stroke, StrokeCap.Round)
        drawLine(colors.borderStrong, Offset(cx, ys[0] - nodeRadius * 1.4f), Offset(cx, 0f), stroke, StrokeCap.Round)
        drawLine(colors.borderStrong, Offset(cx, ys[0] - nodeRadius * 1.4f), Offset(cx + size.width * 0.24f, 0f), stroke, StrokeCap.Round)
        drawLine(colors.accentStrong, Offset(cx, ys[3] + nodeRadius * 1.4f), Offset(cx - size.width * 0.24f, size.height), stroke, StrokeCap.Round)
        drawLine(colors.accentStrong, Offset(cx, ys[3] + nodeRadius * 1.4f), Offset(cx, size.height), stroke, StrokeCap.Round)
        drawLine(colors.accentStrong, Offset(cx, ys[3] + nodeRadius * 1.4f), Offset(cx + size.width * 0.24f, size.height), stroke, StrokeCap.Round)
        val fills = listOf(colors.borderStrong, colors.accentDeep, colors.accentPrimary, colors.accentStrong)
        ys.forEachIndexed { index, y ->
            drawCircle(fills[index], nodeRadius, Offset(cx, y))
            if (index < 3) {
                drawCircle(colors.accentStrong, nodeRadius * 1.35f, Offset(cx, y), style = Stroke(width = stroke * 0.65f))
            }
        }
    }
}
