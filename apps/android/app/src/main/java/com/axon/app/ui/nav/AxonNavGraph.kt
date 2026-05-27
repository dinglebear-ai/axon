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
import com.axon.app.ui.operations.OperationsScreen
import com.axon.app.ui.settings.SettingsScreen
import com.axon.app.ui.system.SystemScreen
import kotlinx.serialization.Serializable
import tv.tootie.aurora.components.AuroraThinking

@Serializable object HomeRoute
@Serializable object SettingsRoute

/** Opens a saved document by URL via /v1/retrieve. */
@Serializable data class DocumentRoute(val url: String)

/** Page labels for the [HorizontalPager] inside [HomeShell]. */
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
    CompositionLocalProvider(LocalAxonNavController provides navController) {
        NavHost(
            navController = navController,
            startDestination = HomeRoute,
        ) {
            composable<HomeRoute>     { HomeShell(navController) }
            composable<SettingsRoute> { SettingsShell(navController) }
            composable<DocumentRoute> { entry ->
                val route: DocumentRoute = entry.toRoute()
                DocumentShell(navController = navController, url = route.url)
            }
        }
    }
}

/**
 * Top-level shell: a TopAppBar with the page title + settings gear, and a
 * HorizontalPager hosting the four primary pages. Swiping switches pages; the
 * bottom bar is intentionally absent — the FAB on the Operations page drives
 * mode selection there.
 */
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
                    IconButton(onClick = { navController.navigate(SettingsRoute) }) {
                        Icon(Icons.Filled.Settings, contentDescription = "Settings")
                    }
                },
            )
        },
    ) { innerPadding ->
        HorizontalPager(
            state = pagerState,
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
private fun SettingsShell(navController: NavController) {
    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Settings", style = MaterialTheme.typography.titleMedium) },
                navigationIcon = {
                    IconButton(onClick = { navController.popBackStack() }) {
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
            SettingsScreen()
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DocumentShell(navController: NavController, url: String) {
    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Document", style = MaterialTheme.typography.titleMedium) },
                navigationIcon = {
                    IconButton(onClick = { navController.popBackStack() }) {
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
            DocumentScreen(url = url)
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
