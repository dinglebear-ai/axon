package com.axon.app.ui.nav

import androidx.compose.runtime.Composable
import androidx.navigation.NavController
import com.axon.app.ui.jobs.JobsDrawerContent
import com.axon.app.ui.knowledge.KnowledgeTab
import com.axon.app.ui.knowledge.KnowledgeDrawerContent
import com.axon.app.ui.management.ManagementDrawerContent
import com.axon.app.ui.sessions.SessionsDrawerContent
import com.axon.app.ui.setup.SetupDrawerContent

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
        DrawerSection.Sessions   -> SessionsDrawerContent(onSelect = { _ -> onDismiss() })
        DrawerSection.Jobs       -> JobsDrawerContent()
        DrawerSection.Knowledge  -> KnowledgeDrawerContent(
            onOpenSuggest = { open(ShellOverlay.Knowledge(KnowledgeTab.Suggest)) },
            onOpenSources = { open(ShellOverlay.Knowledge(KnowledgeTab.Sources)) },
            onOpenDomains = { open(ShellOverlay.Knowledge(KnowledgeTab.Domains)) },
            onOpenStats = { open(ShellOverlay.Knowledge(KnowledgeTab.Stats)) },
        )
        DrawerSection.Management -> ManagementDrawerContent(
            onOpenDedupe = { open(ShellOverlay.Command(ShellCommand.Dedupe)) },
            onOpenMonitor = { open(ShellOverlay.Command(ShellCommand.Monitor)) },
            onOpenSync = { open(ShellOverlay.Command(ShellCommand.Sync)) },
            onOpenStack = { open(ShellOverlay.Command(ShellCommand.Stack)) },
            onOpenSettings = { open(ShellOverlay.Settings) },
        )
        DrawerSection.Setup -> SetupDrawerContent(
            onOpenPreflight = { open(ShellOverlay.Command(ShellCommand.Preflight)) },
            onOpenSetup = { open(ShellOverlay.Command(ShellCommand.Setup)) },
            onOpenSmoke = { open(ShellOverlay.Command(ShellCommand.Smoke)) },
            onOpenDoctor = { open(ShellOverlay.Command(ShellCommand.Doctor)) },
            onOpenDebug = { open(ShellOverlay.Command(ShellCommand.Debug)) },
        )
    }
}
