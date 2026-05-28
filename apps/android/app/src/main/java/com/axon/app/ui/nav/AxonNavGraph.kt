package com.axon.app.ui.nav

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import com.axon.app.AxonApp
import com.axon.app.ui.document.DocumentScreen
import com.axon.app.ui.jobs.JobsScreen
import com.axon.app.ui.knowledge.KnowledgeScreen
import com.axon.app.ui.operations.OperationMode
import com.axon.app.ui.operations.OperationsScreen
import com.axon.app.ui.options.ModeOptionsScreen
import com.axon.app.ui.settings.SettingsScreen
import com.axon.app.ui.status.ConnectionStatusIndicator
import com.axon.app.ui.system.SystemScreen
import kotlinx.serialization.Serializable
import tv.tootie.aurora.components.AuroraThinking

@Serializable object HomeRoute
@Serializable object SettingsRoute

/**
 * Opens a saved document by URL via /v1/retrieve.
 *
 * [url] must be percent-encoded before navigating if it contains characters that
 * Navigation Compose treats as delimiters (e.g. `?`, `&`, `#`). Use
 * `Uri.encode(url)` at the call site and `Uri.decode(url)` in the destination.
 */
@Serializable data class DocumentRoute(val url: String)

/**
 * Opens the mode-options form for [modeName]. The mode name is the enum
 * `OperationMode.name`; we re-resolve via `OperationMode.valueOf(...)` at the
 * destination so we don't have to register a custom `NavType` for the enum.
 *
 * If an unrecognised name slips through (e.g. legacy deep link), the
 * destination logs and pops back via the `?:` fallback.
 */
@Serializable data class ModeOptionsRoute(val modeName: String)

private val PAGES = listOf("Operations", "Jobs", "Knowledge", "System")

@Composable
fun AxonNavGraph() {
    val context = LocalContext.current
    val container = (context.applicationContext as AxonApp).container
    val isReady by container.isReady.collectAsStateWithLifecycle()

    if (!isReady) {
        Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            AuroraThinking(label = "Initializing…")
        }
        return
    }

    val navController = rememberNavController()
    // Stable callback: same lambda identity across recompositions so deep children
    // don't see a new function reference per render.
    val openDocument = remember(navController) {
        { url: String -> navController.navigate(DocumentRoute(url)); Unit }
    }
    val openModeOptions = remember(navController) {
        { mode: OperationMode -> navController.navigate(ModeOptionsRoute(mode.name)); Unit }
    }
    CompositionLocalProvider(
        LocalOpenDocument provides openDocument,
        LocalOpenModeOptions provides openModeOptions,
    ) {
        NavHost(
            navController = navController,
            startDestination = HomeRoute,
        ) {
            composable<HomeRoute>     { HomeShell(navController) }
            composable<SettingsRoute> { BackShell("Settings", navController::popBackStack) { SettingsScreen() } }
            composable<DocumentRoute> { entry ->
                val route: DocumentRoute = entry.toRoute()
                BackShell("Document", navController::popBackStack) { DocumentScreen(url = route.url) }
            }
            composable<ModeOptionsRoute> { entry ->
                val route: ModeOptionsRoute = entry.toRoute()
                val mode = runCatching { OperationMode.valueOf(route.modeName) }.getOrNull()
                if (mode == null) {
                    // Unknown mode name — bounce back. Cheaper than a crash dialog.
                    LaunchedPopBack(navController)
                } else {
                    BackShell(
                        title = "${mode.label} options",
                        onBack = navController::popBackStack,
                    ) { ModeOptionsScreen(mode) }
                }
            }
        }
    }
}

@Composable
private fun LaunchedPopBack(navController: NavController) {
    androidx.compose.runtime.LaunchedEffect(Unit) { navController.popBackStack() }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HomeShell(navController: NavController) {
    val pagerState = rememberPagerState(pageCount = { PAGES.size })

    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(
                            PAGES[pagerState.currentPage],
                            style = MaterialTheme.typography.titleMedium,
                        )
                        PagerDots(currentPage = pagerState.currentPage, total = PAGES.size)
                    }
                },
                actions = {
                    ConnectionStatusIndicator()
                    IconButton(onClick = { navController.navigate(SettingsRoute) }) {
                        Icon(Icons.Filled.Settings, contentDescription = "Settings")
                    }
                },
            )
        },
    ) { innerPadding ->
        HorizontalPager(
            state = pagerState,
            // Don't eagerly compose neighbour pages — each one resolves its own
            // ViewModel(s) on first composition and we want that deferred until
            // the user actually swipes there.
            beyondViewportPageCount = 0,
            modifier = Modifier
                .padding(innerPadding)
                .consumeWindowInsets(innerPadding)
                .imePadding()
                .fillMaxSize(),
        ) { page ->
            when (page) {
                0 -> OperationsScreen()
                1 -> JobsScreen()
                2 -> KnowledgeScreen()
                3 -> SystemScreen()
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun BackShell(
    title: String,
    onBack: () -> Unit,
    content: @Composable () -> Unit,
) {
    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(title, style = MaterialTheme.typography.titleMedium) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { innerPadding ->
        Box(
            modifier = Modifier
                .padding(innerPadding)
                .consumeWindowInsets(innerPadding)
                .imePadding()
                .fillMaxSize(),
        ) {
            content()
        }
    }
}

@Composable
private fun RowScope.PagerDots(currentPage: Int, total: Int) {
    repeat(total) { i ->
        val selected = i == currentPage
        Surface(
            modifier = Modifier
                .padding(horizontal = 2.dp)
                .size(if (selected) 8.dp else 6.dp),
            shape = CircleShape,
            color = if (selected) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.outlineVariant
            },
        ) {}
    }
}
