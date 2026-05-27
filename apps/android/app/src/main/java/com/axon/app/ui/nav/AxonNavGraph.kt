package com.axon.app.ui.nav

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.QuestionAnswer
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavDestination.Companion.hasRoute
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.axon.app.AxonApp
import com.axon.app.ui.ask.AskScreen
import com.axon.app.ui.search.SearchScreen
import com.axon.app.ui.settings.SettingsScreen
import com.axon.app.ui.sources.SourcesScreen
import com.axon.app.ui.tools.ToolsScreen
import kotlinx.serialization.Serializable

@Serializable object AskRoute
@Serializable object SearchRoute
@Serializable object ToolsRoute
@Serializable object SourcesRoute
@Serializable object SettingsRoute

private data class NavItem(
    val label: String,
    val icon: ImageVector,
    val route: Any,
)

private val navItems = listOf(
    NavItem("Ask",      Icons.Default.QuestionAnswer, AskRoute),
    NavItem("Search",   Icons.Default.Hub,            SearchRoute),
    NavItem("Tools",    Icons.Default.Build,          ToolsRoute),
    NavItem("Sources",  Icons.Default.List,           SourcesRoute),
    NavItem("Settings", Icons.Default.Settings,       SettingsRoute),
)

@Composable
fun AxonNavGraph() {
    val context = LocalContext.current
    val container = (context.applicationContext as AxonApp).container
    val isReady by container.isReady.collectAsStateWithLifecycle()

    if (!isReady) {
        Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            CircularProgressIndicator()
        }
        return
    }

    val navController = rememberNavController()
    val backStackEntry by navController.currentBackStackEntryAsState()
    val currentDest = backStackEntry?.destination

    Scaffold(
        bottomBar = {
            NavigationBar {
                navItems.forEach { item ->
                    NavigationBarItem(
                        selected = currentDest?.hasRoute(item.route::class) == true,
                        onClick = {
                            navController.navigate(item.route) {
                                popUpTo(navController.graph.findStartDestination().id) {
                                    saveState = true
                                }
                                launchSingleTop = true
                                restoreState = true
                            }
                        },
                        icon = { Icon(item.icon, contentDescription = item.label) },
                        label = { Text(item.label) },
                    )
                }
            }
        }
    ) { innerPadding ->
        NavHost(
            navController = navController,
            startDestination = AskRoute,
            modifier = Modifier.padding(innerPadding),
        ) {
            composable<AskRoute>     { AskScreen() }
            composable<SearchRoute>  { SearchScreen() }
            composable<ToolsRoute>   { ToolsScreen() }
            composable<SourcesRoute> { SourcesScreen() }
            composable<SettingsRoute>{ SettingsScreen() }
        }
    }
}
