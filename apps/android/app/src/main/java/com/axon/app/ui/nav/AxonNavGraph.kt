package com.axon.app.ui.nav

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import com.axon.app.AxonApp
import com.axon.app.ui.document.DocumentScreen
import com.axon.app.ui.knowledge.SuggestScreen
import com.axon.app.ui.operations.OperationMode
import com.axon.app.ui.options.ModeOptionsScreen
import com.axon.app.ui.settings.SettingsScreen
import kotlinx.serialization.Serializable
import tv.tootie.aurora.components.AuroraThinking

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
    CompositionLocalProvider(
        LocalOpenDocument provides openDocument,
    ) {
        NavHost(
            navController = navController,
            startDestination = RailShellRoute,
        ) {
            composable<RailShellRoute>  { RailScaffold(navController = navController) }
            composable<SettingsRoute>   { BackShell("Settings", navController::popBackStack) { SettingsScreen() } }
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
            composable<SuggestRoute> {
                BackShell("Suggest", navController::popBackStack) {
                    SuggestScreen()
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
internal fun BackShell(
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
