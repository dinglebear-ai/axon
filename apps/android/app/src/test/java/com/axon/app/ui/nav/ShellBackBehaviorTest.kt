package com.axon.app.ui.nav

import org.junit.Assert.assertEquals
import org.junit.Test

class ShellBackBehaviorTest {
    @Test
    fun `nested page state gets first chance to handle back`() {
        val target = resolveShellBackTarget(
            activeOverlay = false,
            sidebarOpen = false,
            activePage = DrawerSection.Activity,
            childCanHandleBack = true,
            askReturnPage = null,
        )

        assertEquals(ShellBackTarget.Child, target)
    }

    @Test
    fun `ask opened from a page returns to that page before leaving shell`() {
        val target = resolveShellBackTarget(
            activeOverlay = false,
            sidebarOpen = false,
            activePage = null,
            childCanHandleBack = false,
            askReturnPage = DrawerSection.Sessions,
        )

        assertEquals(ShellBackTarget.ReturnToPage(DrawerSection.Sessions), target)
    }

    @Test
    fun `plain top level page backs to ask`() {
        val target = resolveShellBackTarget(
            activeOverlay = false,
            sidebarOpen = false,
            activePage = DrawerSection.Jobs,
            childCanHandleBack = false,
            askReturnPage = null,
        )

        assertEquals(ShellBackTarget.Ask, target)
    }
}
