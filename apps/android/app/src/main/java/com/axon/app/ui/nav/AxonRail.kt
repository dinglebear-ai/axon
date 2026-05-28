package com.axon.app.ui.nav

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private val NavBg         = Color(0xFF07111A)
private val AccentPrimary = Color(0xFF29B6F6)
private val TextMuted     = Color(0xFFA7BCC9)

private data class SectionDef(val section: DrawerSection, val icon: ImageVector, val label: String)

private val TopSections = listOf(
    SectionDef(DrawerSection.Sessions,   Icons.Rounded.History,      "Sess"),
    SectionDef(DrawerSection.Jobs,       Icons.Rounded.Checklist,    "Jobs"),
    SectionDef(DrawerSection.Knowledge,  Icons.Rounded.Hub,          "Know"),
    SectionDef(DrawerSection.Management, Icons.Rounded.Settings,     "Mgmt"),
)
private val BottomSections = listOf(
    SectionDef(DrawerSection.Setup, Icons.Rounded.Construction, "Setup"),
)

@Composable
fun AxonRail(
    activeSection: DrawerSection?,
    onSectionClick: (DrawerSection) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .width(54.dp)
            .fillMaxHeight()
            .background(NavBg),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Spacer(Modifier.height(10.dp))
        TopSections.forEach { def ->
            RailItem(def.icon, def.label, activeSection == def.section) { onSectionClick(def.section) }
        }
        Spacer(Modifier.weight(1f))
        BottomSections.forEach { def ->
            RailItem(def.icon, def.label, activeSection == def.section) { onSectionClick(def.section) }
        }
        Spacer(Modifier.height(10.dp))
    }
}

@Composable
private fun RailItem(icon: ImageVector, label: String, active: Boolean, onClick: () -> Unit) {
    val tint = if (active) AccentPrimary else TextMuted
    Box(
        modifier = Modifier
            .size(46.dp, 42.dp)
            .clip(RoundedCornerShape(13.dp))
            .background(if (active) AccentPrimary.copy(alpha = 0.12f) else Color.Transparent)
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick),
    ) {
        if (active) {
            Box(
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    .width(3.dp)
                    .height(22.dp)
                    .clip(RoundedCornerShape(topEnd = 2.dp, bottomEnd = 2.dp))
                    .background(AccentPrimary),
            )
        }
        Column(
            modifier = Modifier.align(Alignment.Center),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Icon(imageVector = icon, contentDescription = label, tint = tint, modifier = Modifier.size(20.dp))
            Text(label.uppercase(), fontSize = 7.sp, fontWeight = FontWeight.SemiBold, color = tint, letterSpacing = 0.5.sp)
        }
    }
}
