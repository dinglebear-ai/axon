package com.axon.app.ui.nav

import androidx.compose.runtime.Composable
import androidx.navigation.NavController
import com.axon.app.feature.jobs.ActivityHistoryScreen
import com.axon.app.feature.jobs.JobsDrawerContent
import com.axon.app.feature.memory.KnowledgeTab
import com.axon.app.feature.memory.KnowledgeDrawerContent
import com.axon.app.ui.sessions.SessionsDrawerContent

@Composable
fun DrawerSectionContent(
    section: DrawerSection,
    onDismiss: () -> Unit,
    navController: NavController,
    onOpenOverlay: (ShellOverlay) -> Unit,
) {
    fun open(overlay: ShellOverlay) {
        onDismiss()
        onOpenOverlay(overlay)
    }

    when (section) {
        DrawerSection.Activity -> ActivityHistoryScreen()
        DrawerSection.Sessions   -> SessionsDrawerContent(onSelect = { _ -> onDismiss() })
        DrawerSection.Jobs       -> JobsDrawerContent()
        DrawerSection.Knowledge  -> KnowledgeDrawerContent(
            onOpenSuggest = { open(ShellOverlay.Knowledge(KnowledgeTab.Suggest)) },
            onOpenSources = { open(ShellOverlay.Knowledge(KnowledgeTab.Sources)) },
            onOpenDomains = { open(ShellOverlay.Knowledge(KnowledgeTab.Domains)) },
            onOpenStats = { open(ShellOverlay.Knowledge(KnowledgeTab.Stats)) },
        )
        DrawerSection.Settings -> Unit
    }
}
