package com.axon.app.ui.nav

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.navigation.NavController
import com.axon.app.ui.ask.AskScreen

@Composable
fun RailScaffold(navController: NavController, modifier: Modifier = Modifier) {
    var activeSection by remember { mutableStateOf<DrawerSection?>(null) }

    BackHandler(enabled = activeSection != null) { activeSection = null }

    Row(
        modifier = modifier
            .fillMaxSize()
            .background(Color(0xFF07131C))
    ) {
        AxonRail(
            activeSection = activeSection,
            onSectionClick = { section ->
                activeSection = if (activeSection == section) null else section
            },
        )

        Box(modifier = Modifier.weight(1f)) {
            AskScreen(
                onOpenDocument = { url -> navController.navigate(DocumentRoute(url)) },
            )
        }

        AnimatedVisibility(
            visible = activeSection != null,
            enter = slideInHorizontally(tween(220)) { -it } + fadeIn(tween(180)),
            exit  = slideOutHorizontally(tween(180)) { -it } + fadeOut(tween(150)),
            modifier = Modifier.fillMaxSize(),
        ) {
            OverlayDrawer(
                section = activeSection ?: DrawerSection.Sessions,
                onDismiss = { activeSection = null },
                navController = navController,
            )
        }
    }
}
