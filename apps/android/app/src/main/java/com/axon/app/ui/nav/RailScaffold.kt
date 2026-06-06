package com.axon.app.ui.nav

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
import androidx.compose.material.icons.rounded.Construction
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.History
import androidx.compose.material.icons.rounded.Home
import androidx.compose.material.icons.rounded.Hub
import androidx.compose.material.icons.rounded.Menu
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.TaskAlt
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.Text
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
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
import com.axon.app.ui.jobs.JobsScreen
import com.axon.app.ui.knowledge.KnowledgeScreen
import com.axon.app.ui.knowledge.KnowledgeTab
import com.axon.app.ui.management.ManagementDrawerContent
import com.axon.app.ui.management.ManagementViewModel
import com.axon.app.ui.sessions.SessionsDrawerContent
import com.axon.app.ui.settings.SettingsScreen
import com.axon.app.ui.setup.SetupDrawerContent
import com.axon.app.ui.setup.SetupViewModel
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import kotlinx.coroutines.launch

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
    Monitor("Monitor", "status", "live job + GPU monitor"),
    Sync("Sync", "watch", "sitemap backfill"),
    Stack("Stack", "stats", "compose services"),
    Preflight("Preflight", "smoke + doctor", "check prerequisites"),
    Setup("Setup", "setup", "init + compose up"),
    Smoke("Smoke", "healthz", "crawl/ask proof"),
    Doctor("Doctor", "doctor", "service health"),
    Debug("Debug", "debug", "env + paths"),
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
    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
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
        scope.launch { drawerState.close() }
    }

    BackHandler(enabled = activeOverlay != null || drawerState.isOpen || activePage != null) {
        if (activeOverlay != null) {
            activeOverlay = null
        } else if (drawerState.isOpen) {
            scope.launch { drawerState.close() }
        } else {
            activePage = null
        }
    }

    ModalNavigationDrawer(
        drawerState = drawerState,
        drawerContent = {
            AxonSidebarSheet(
                items = sidebarItems,
                selected = selectedValue(),
                onSelect = ::selectSidebarValue,
            )
        },
        modifier = modifier.fillMaxSize(),
        scrimColor = colors.pageBg.copy(alpha = 0.72f),
    ) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(colors.pageBg)
            ) {
                Column(modifier = Modifier.fillMaxSize().statusBarsPadding()) {
                    AxonTopBar(
                        title = activeOverlay?.title ?: activePage?.title() ?: "Ask",
                        sidebarOpen = drawerState.isOpen,
                        onToggleSidebar = {
                            scope.launch {
                                if (drawerState.isOpen) drawerState.close() else drawerState.open()
                            }
                        },
                    )
                    Box(Modifier.fillMaxWidth().height(1.dp).background(colors.borderDefault))
                    Box(modifier = Modifier.weight(1f).fillMaxWidth().clip(RoundedCornerShape(0.dp))) {
                        ShellPageContent(
                            page = activePage,
                            navController = navController,
                            onOpenOverlay = { activeOverlay = it },
                        )
                        activeOverlay?.let { overlay ->
                            ShellOverlayContent(
                                overlay = overlay,
                                onBack = { activeOverlay = null },
                                onHome = {
                                    activeOverlay = null
                                    activePage = null
                                },
                            )
                        }
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
            .width(244.dp)
            .fillMaxHeight()
            .background(colors.panelStrong)
            .border(width = 1.dp, color = colors.borderDefault)
            .statusBarsPadding()
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
        null -> AskScreen(onOpenDocument = { url -> navController.navigate(DocumentRoute(url)) })
        DrawerSection.Sessions -> PageSurface { SessionsDrawerContent() }
        DrawerSection.Jobs -> JobsScreen()
        DrawerSection.Knowledge -> KnowledgeScreen()
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
            .padding(8.dp),
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
                .height(48.dp)
                .background(colors.panelMedium)
                .border(1.dp, colors.borderDefault)
                .padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(
                Icons.AutoMirrored.Rounded.ArrowBack,
                contentDescription = "Back",
                tint = colors.textMuted,
                modifier = Modifier
                    .size(28.dp)
                    .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onBack)
                    .padding(5.dp),
            )
            Text(
                overlay.title,
                color = colors.textPrimary,
                fontSize = 15.sp,
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
                    .size(28.dp)
                    .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onHome)
                    .padding(5.dp),
            )
        }
        Box(modifier = Modifier.fillMaxSize()) {
            when (overlay) {
                is ShellOverlay.Knowledge -> KnowledgeScreen(initialTab = overlay.tab, showChrome = false)
                ShellOverlay.Settings -> SettingsScreen()
                is ShellOverlay.Command -> ShellCommandReport(command = overlay.command)
            }
        }
    }
}

@Composable
private fun ShellCommandReport(
    command: ShellCommand,
    setupVm: SetupViewModel = viewModel(),
    managementVm: ManagementViewModel = viewModel(),
) {
    val colors = AxonTheme.colors
    val smoke by setupVm.smokeState.collectAsStateWithLifecycle()
    val doctor by setupVm.doctorState.collectAsStateWithLifecycle()
    val stack by managementVm.statsState.collectAsStateWithLifecycle()

    LaunchedEffect(command) {
        when (command) {
            ShellCommand.Preflight -> {
                setupVm.runSmoke()
                setupVm.runDoctor()
            }
            ShellCommand.Smoke -> setupVm.runSmoke()
            ShellCommand.Doctor -> setupVm.runDoctor()
            ShellCommand.Stack, ShellCommand.Monitor -> managementVm.loadStats()
            else -> Unit
        }
    }

    val status = when (command) {
        ShellCommand.Preflight -> combineStatus(smoke, doctor)
        ShellCommand.Smoke -> resourceStatus(smoke)
        ShellCommand.Doctor -> resourceStatus(doctor)
        ShellCommand.Stack, ShellCommand.Monitor -> resourceStatus(stack)
        ShellCommand.Dedupe, ShellCommand.Sync, ShellCommand.Setup, ShellCommand.Debug -> "READY"
    }
    val output = when (command) {
        ShellCommand.Preflight -> listOf(
            "healthz: ${resourceLine(smoke)}",
            "doctor: ${resourceLine(doctor)}",
        ).joinToString("\n")
        ShellCommand.Smoke -> resourceLine(smoke)
        ShellCommand.Doctor -> resourceLine(doctor)
        ShellCommand.Stack, ShellCommand.Monitor -> resourceLine(stack)
        ShellCommand.Dedupe -> "dedupe service command is not exposed by the Android API yet"
        ShellCommand.Sync -> "watch/sitemap sync command is not exposed by the Android API yet"
        ShellCommand.Setup -> "configure .env and config.toml from Management > Config"
        ShellCommand.Debug -> "debug report uses doctor plus config paths; open Doctor and Config for live values"
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            "COMMAND REPORT",
            color = colors.textMuted,
            fontSize = 10.sp,
            fontWeight = FontWeight.ExtraBold,
            fontFamily = AxonTheme.fonts.mono,
            letterSpacing = 0.4.sp,
        )
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(16.dp))
                .background(colors.panelStrong)
                .border(1.dp, colors.borderStrong, RoundedCornerShape(16.dp))
                .padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Box(
                    modifier = Modifier
                        .size(38.dp)
                        .clip(RoundedCornerShape(12.dp))
                        .background(colors.tint(colors.accentPrimary, 12, colors.panelStrong))
                        .border(1.dp, colors.tint(colors.accentPrimary, 30, colors.panelStrong), RoundedCornerShape(12.dp)),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Rounded.Settings, contentDescription = null, tint = colors.accentStrong, modifier = Modifier.size(19.dp))
                }
                Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                    Text(command.title, color = colors.textPrimary, fontSize = 16.sp, fontWeight = FontWeight.ExtraBold, fontFamily = AxonTheme.fonts.display)
                    Text(command.summary, color = colors.textMuted, fontSize = 11.5.sp, fontFamily = AxonTheme.fonts.body)
                }
                Text(
                    status,
                    color = if (status == "ERROR") colors.error else colors.success,
                    fontSize = 9.sp,
                    fontWeight = FontWeight.Bold,
                    fontFamily = AxonTheme.fonts.mono,
                    modifier = Modifier
                        .border(1.dp, colors.tint(if (status == "ERROR") colors.error else colors.success, 34, colors.panelStrong), RoundedCornerShape(5.dp))
                        .padding(horizontal = 6.dp, vertical = 2.dp),
                )
            }
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(13.dp))
                    .background(colors.control)
                    .border(1.dp, colors.borderDefault, RoundedCornerShape(13.dp))
                    .padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                Text("$ ${command.endpoint}", color = colors.accentStrong, fontSize = 11.sp, fontFamily = AxonTheme.fonts.mono)
                Text(output, color = colors.textPrimary, fontSize = 11.sp, fontFamily = AxonTheme.fonts.mono)
            }
        }
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

@Composable
private fun AxonTopBar(
    title: String,
    sidebarOpen: Boolean,
    onToggleSidebar: () -> Unit,
) {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(50.dp)
            .background(colors.navBg)
            .padding(start = 14.dp, end = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            Icons.Rounded.Menu,
            contentDescription = if (sidebarOpen) "Collapse sidebar" else "Open sidebar",
            tint = colors.textMuted,
            modifier = Modifier
                .size(30.dp)
                .clip(RoundedCornerShape(9.dp))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onToggleSidebar)
                .padding(6.dp),
        )
        Spacer(Modifier.width(8.dp))
        AxonMarkGlyph(modifier = Modifier.size(22.dp))
        Spacer(Modifier.width(8.dp))
        Text("Axon", color = colors.textPrimary, fontSize = 15.5.sp, fontWeight = FontWeight.ExtraBold, fontFamily = AxonTheme.fonts.display)
        Spacer(Modifier.width(8.dp))
        AuroraStatusDot(DotState.Done, size = 6.dp)
        Box(modifier = Modifier.weight(1f), contentAlignment = Alignment.Center) {
            Text(title, color = colors.textPrimary, fontSize = 13.sp, fontWeight = FontWeight.ExtraBold, fontFamily = AxonTheme.fonts.display)
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            AuroraStatusDot(DotState.Done, size = 7.dp)
            Text("LIVE", color = colors.textMuted, fontSize = 11.sp, fontFamily = AxonTheme.fonts.mono)
        }
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
