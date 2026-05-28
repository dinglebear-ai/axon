package com.axon.app.ui.nav

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation.NavController
import com.axon.app.ui.jobs.JobsDrawerContent
import com.axon.app.ui.sessions.SessionsDrawerContent

@Composable
fun DrawerSectionContent(
    section: DrawerSection,
    onDismiss: () -> Unit,
    navController: NavController,
) {
    when (section) {
        DrawerSection.Sessions   -> SessionsDrawerContent(onSelect = { _ -> onDismiss() })
        DrawerSection.Jobs       -> JobsDrawerContent()
        DrawerSection.Knowledge  -> KnowledgeDrawerContentStub()
        DrawerSection.Management -> ManagementDrawerContentStub()
        DrawerSection.Setup      -> SetupDrawerContentStub()
    }
}

// Stubs — replaced in later tasks
@Composable private fun KnowledgeDrawerContentStub() =
    DrawerStub(Icons.Rounded.Hub, "Knowledge")
@Composable private fun ManagementDrawerContentStub() =
    DrawerStub(Icons.Rounded.Settings, "Management")
@Composable private fun SetupDrawerContentStub() =
    DrawerStub(Icons.Rounded.Construction, "Setup")

@Composable
private fun DrawerStub(icon: ImageVector, title: String) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(14.dp, 14.dp, 14.dp, 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(imageVector = icon, contentDescription = title, tint = Color(0xFF29B6F6), modifier = Modifier.size(18.dp))
        Text(title, fontSize = 14.sp, fontWeight = FontWeight.Bold, color = Color(0xFFE6F4FB))
    }
}
