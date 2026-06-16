package com.axon.app.ui.nav

import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Construction
import androidx.compose.material.icons.rounded.History
import androidx.compose.material.icons.rounded.Home
import androidx.compose.material.icons.rounded.Hub
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.material.icons.rounded.TaskAlt
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation.NavController
import com.axon.app.ui.ask.AskScreen
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.jobs.JobsScreen
import com.axon.app.ui.knowledge.KnowledgeScreen
import com.axon.app.ui.knowledge.KnowledgeTab
import com.axon.app.ui.management.ManagementDrawerContent
import com.axon.app.ui.sessions.SessionsDrawerContent
import com.axon.app.ui.settings.SettingsScreen
import com.axon.app.ui.status.TopChromeStatus
import com.axon.app.ui.setup.SetupDrawerContent
import com.axon.app.ui.theme.AxonTheme

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

/** Sentinel for the default "Ask" home page (no active drawer section). */
private data object ShellHome

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
                overlayActive = activeOverlay != null,
                sidebarOpen = sidebarOpen,
                onToggleSidebar = { sidebarOpen = !sidebarOpen },
                onCloseOverlay = { activeOverlay = null },
            )
            Box(Modifier.fillMaxWidth().height(1.dp).background(colors.borderDefault.copy(alpha = 0.32f)))
            Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
                // Fade + gentle rise whenever the visible surface changes — page
                // switches and overlay open/close stop being a hard cut.
                AnimatedContent(
                    targetState = activeOverlay ?: activePage ?: ShellHome,
                    transitionSpec = {
                        val enter = fadeIn(tween(durationMillis = 220, delayMillis = 24)) +
                            slideInVertically(tween(durationMillis = 300)) { full -> full / 18 }
                        val exit = fadeOut(tween(durationMillis = 150))
                        enter togetherWith exit
                    },
                    modifier = Modifier.fillMaxSize(),
                    label = "shell-content",
                ) { target ->
                    when (target) {
                        is ShellOverlay -> ShellOverlayContent(
                            overlay = target,
                            navController = navController,
                        )
                        is DrawerSection -> ShellPageContent(
                            page = target,
                            navController = navController,
                            onOpenOverlay = ::openOverlay,
                        )
                        else -> ShellPageContent(
                            page = null,
                            navController = navController,
                            onOpenOverlay = ::openOverlay,
                        )
                    }
                }
                ShellSidebarOverlay(
                    open = sidebarOpen,
                    items = sidebarItems,
                    selected = selectedValue(),
                    onSelect = ::selectSidebarValue,
                    onScrimClick = { sidebarOpen = false },
                )
            }
        }
    }
}

/**
 * Scrim + sidebar sheet that springs in from the left edge. Stays composed
 * through the close animation (until progress fully settles to 0) so dismissal
 * glides out instead of vanishing.
 */
@Composable
private fun BoxScope.ShellSidebarOverlay(
    open: Boolean,
    items: List<SidebarItem>,
    selected: String,
    onSelect: (String) -> Unit,
    onScrimClick: () -> Unit,
) {
    val progress by animateFloatAsState(
        targetValue = if (open) 1f else 0f,
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioNoBouncy,
            stiffness = Spring.StiffnessMediumLow,
        ),
        label = "sidebar-open",
    )
    if (!open && progress < 0.001f) return

    val slidePx = with(LocalDensity.current) { SidebarSheetWidth.toPx() }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .graphicsLayer { alpha = progress }
            .background(MaterialTheme.colorScheme.scrim.copy(alpha = 0.50f))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onScrimClick),
    )
    AxonSidebarSheet(
        items = items,
        selected = selected,
        onSelect = onSelect,
        modifier = Modifier.graphicsLayer { translationX = -slidePx * (1f - progress) },
    )
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
) {
    val colors = AxonTheme.colors
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(colors.pageBg),
    ) {
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

/**
 * Single shell header. On a page/home it shows the menu + brand + live status;
 * inside an overlay it morphs into a focused back/title/close bar — so an
 * overlay no longer stacks a second redundant header beneath this one.
 */
@Composable
private fun AxonTopBar(
    title: String,
    overlayActive: Boolean,
    sidebarOpen: Boolean,
    onToggleSidebar: () -> Unit,
    onCloseOverlay: () -> Unit,
) {
    val colors = AxonTheme.colors
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(52.dp)
            .background(colors.navBg)
            .padding(horizontal = 8.dp),
    ) {
        // Sidebar toggle + brand — present on every screen, overlays included.
        Row(
            modifier = Modifier.align(Alignment.CenterStart),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                modifier = Modifier
                    .size(38.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .pressScale(onClick = onToggleSidebar)
                    .semantics { contentDescription = if (sidebarOpen) "Collapse sidebar" else "Open sidebar" }
                    .padding(7.dp),
                contentAlignment = Alignment.Center,
            ) {
                AxonMarkGlyph(Modifier.fillMaxSize())
            }
        }
        Text(
            title,
            color = colors.textPrimary.copy(alpha = 0.95f),
            fontSize = 16.sp,
            fontWeight = FontWeight.ExtraBold,
            fontFamily = AxonTheme.fonts.display,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .align(Alignment.Center)
                .widthIn(max = 200.dp),
        )
        Box(modifier = Modifier.align(Alignment.CenterEnd)) {
            if (overlayActive) {
                Icon(
                    Icons.Rounded.Close,
                    contentDescription = "Close",
                    tint = colors.textMuted,
                    modifier = Modifier
                        .size(38.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .pressScale(onClick = onCloseOverlay)
                        .padding(8.dp),
                )
            } else {
                TopChromeStatus()
            }
        }
    }
}
