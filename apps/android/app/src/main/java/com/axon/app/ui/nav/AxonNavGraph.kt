package com.axon.app.ui.nav

import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import com.axon.app.AxonApp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.document.DocumentScreen
import com.axon.app.ui.knowledge.SuggestScreen
import com.axon.app.ui.operations.OperationMode
import com.axon.app.ui.options.ModeOptionsScreen
import com.axon.app.ui.settings.SettingsScreen
import com.axon.app.ui.status.TopChromeStatus
import com.axon.app.ui.theme.AxonTheme
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import tv.tootie.aurora.components.AuroraThinking

@SerialName("settings")
@Serializable object SettingsRoute

/**
 * Opens a saved document by URL via /v1/retrieve.
 *
 * Callers pass a raw URL. The shared opener percent-encodes before navigating
 * because URLs often contain delimiters (`?`, `&`, `#`) that Navigation Compose
 * otherwise treats as route syntax.
 */
@SerialName("document")
@Serializable data class DocumentRoute(val url: String)

/**
 * Opens the mode-options form for [modeName]. The mode name is the enum
 * `OperationMode.name`; we re-resolve via `OperationMode.valueOf(...)` at the
 * destination so we don't have to register a custom `NavType` for the enum.
 *
 * If an unrecognised name slips through (e.g. legacy deep link), the
 * destination logs and pops back via the `?:` fallback.
 */
@SerialName("mode_options")
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
        { url: String -> navController.navigate(DocumentRoute(Uri.encode(url))); Unit }
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
                BackShell("Document", navController::popBackStack) { DocumentScreen(url = Uri.decode(route.url)) }
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

@Composable
internal fun BackShell(
    title: String,
    onBack: () -> Unit,
    content: @Composable () -> Unit,
) {
    val colors = AxonTheme.colors
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(colors.pageBg)
            .statusBarsPadding(),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(56.dp)
                .background(colors.navBg)
                .padding(start = 14.dp, end = 12.dp),
        ) {
            Icon(
                Icons.AutoMirrored.Rounded.ArrowBack,
                contentDescription = "Back",
                tint = colors.textMuted,
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    .size(34.dp)
                    .pressScale(onClick = onBack)
                    .clip(RoundedCornerShape(10.dp))
                    .padding(7.dp),
            )
            Text(
                title,
                color = colors.textPrimary.copy(alpha = 0.94f),
                fontSize = 16.sp,
                lineHeight = 21.sp,
                fontWeight = FontWeight.ExtraBold,
                fontFamily = AxonTheme.fonts.display,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier
                    .align(Alignment.Center)
                    .widthIn(max = 220.dp),
            )
            TopChromeStatus(modifier = Modifier.align(Alignment.CenterEnd))
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(colors.borderDefault))
        Box(
            modifier = Modifier
                .imePadding()
                .fillMaxSize(),
        ) {
            content()
        }
    }
}
