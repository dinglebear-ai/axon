package com.axon.app.ui.nav

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.History
import androidx.compose.material.icons.rounded.Hub
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.TaskAlt
import androidx.compose.material.icons.rounded.Construction
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation.NavController
import com.axon.app.ui.theme.AxonTheme

@Composable
fun OverlayDrawer(
    section: DrawerSection,
    onDismiss: () -> Unit,
    navController: NavController,
    onOpenOverlay: (ShellOverlay) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val dimens = AxonTheme.dimens
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
                .width(dimens.drawerWidth)
                .fillMaxHeight()
                .background(colors.panelStrong)
                .border(width = 1.dp, color = colors.borderStrong)
                .shadow(elevation = 18.dp)
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = {}),
        ) {
            DrawerHeader(section = section, onDismiss = onDismiss)
            DrawerSectionContent(
                section = section,
                onDismiss = onDismiss,
                navController = navController,
                onOpenOverlay = onOpenOverlay,
            )
        }
    }
}

@Composable
private fun DrawerHeader(section: DrawerSection, onDismiss: () -> Unit) {
    val colors = AxonTheme.colors
    val (title, icon) = when (section) {
        DrawerSection.Sessions -> "Sessions" to Icons.Rounded.History
        DrawerSection.Jobs -> "Jobs" to Icons.Rounded.TaskAlt
        DrawerSection.Knowledge -> "Knowledge" to Icons.Rounded.Hub
        DrawerSection.Management -> "Management" to Icons.Rounded.Settings
        DrawerSection.Setup -> "Setup" to Icons.Rounded.Construction
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(48.dp)
            .border(width = 1.dp, color = colors.borderDefault)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        Icon(icon, contentDescription = null, tint = colors.accentStrong, modifier = Modifier.size(17.dp))
        Text(
            title,
            color = colors.textPrimary,
            fontSize = 14.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = AxonTheme.fonts.display,
            modifier = Modifier.weight(1f),
        )
        Icon(
            Icons.Rounded.Close,
            contentDescription = "Close drawer",
            tint = colors.textMuted,
            modifier = Modifier
                .size(26.dp)
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss)
                .padding(5.dp),
        )
    }
}
