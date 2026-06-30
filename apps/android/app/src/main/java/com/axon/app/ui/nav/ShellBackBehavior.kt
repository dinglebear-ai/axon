package com.axon.app.ui.nav

internal sealed interface ShellBackTarget {
    data object None : ShellBackTarget
    data object Child : ShellBackTarget
    data object Overlay : ShellBackTarget
    data object Sidebar : ShellBackTarget
    data class ReturnToPage(val page: DrawerSection) : ShellBackTarget
    data object Ask : ShellBackTarget
}

internal fun resolveShellBackTarget(
    activeOverlay: Boolean,
    sidebarOpen: Boolean,
    activePage: DrawerSection?,
    childCanHandleBack: Boolean,
    askReturnPage: DrawerSection?,
): ShellBackTarget = when {
    activeOverlay -> ShellBackTarget.Overlay
    sidebarOpen -> ShellBackTarget.Sidebar
    childCanHandleBack -> ShellBackTarget.Child
    askReturnPage != null -> ShellBackTarget.ReturnToPage(askReturnPage)
    activePage != null -> ShellBackTarget.Ask
    else -> ShellBackTarget.None
}
