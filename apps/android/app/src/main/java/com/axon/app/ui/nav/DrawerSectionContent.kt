package com.axon.app.ui.nav

import androidx.compose.runtime.Composable
import androidx.navigation.NavController
import com.axon.app.ui.jobs.JobsDrawerContent
import com.axon.app.ui.knowledge.KnowledgeDrawerContent
import com.axon.app.ui.management.ManagementDrawerContent
import com.axon.app.ui.sessions.SessionsDrawerContent
import com.axon.app.ui.setup.SetupDrawerContent

@Composable
fun DrawerSectionContent(
    section: DrawerSection,
    onDismiss: () -> Unit,
    navController: NavController,
) {
    when (section) {
        DrawerSection.Sessions   -> SessionsDrawerContent(onSelect = { _ -> onDismiss() })
        DrawerSection.Jobs       -> JobsDrawerContent()
        DrawerSection.Knowledge  -> KnowledgeDrawerContent(onOpenSuggest = { onDismiss(); navController.navigate(SuggestRoute) })
        DrawerSection.Management -> ManagementDrawerContent(
            onOpenSettings = { onDismiss(); navController.navigate(SettingsRoute) },
        )
        DrawerSection.Setup -> SetupDrawerContent(
            onOpenSettings = { onDismiss(); navController.navigate(SettingsRoute) },
        )
    }
}
