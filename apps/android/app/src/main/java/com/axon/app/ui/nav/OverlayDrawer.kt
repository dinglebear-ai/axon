package com.axon.app.ui.nav

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.navigation.NavController

@Composable
fun OverlayDrawer(
    section: DrawerSection,
    onDismiss: () -> Unit,
    navController: NavController,
    modifier: Modifier = Modifier,
) {
    Box(modifier = modifier.fillMaxSize()) {
        // Dim backdrop — tap to dismiss
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Color(0xAE040A0E))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
        )
        // Drawer panel — consumes its own clicks
        Column(
            modifier = Modifier
                .width(232.dp)
                .fillMaxHeight()
                .background(Color(0xFF13293A))
                .statusBarsPadding()
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = {}),
        ) {
            DrawerSectionContent(section = section, onDismiss = onDismiss, navController = navController)
        }
    }
}
