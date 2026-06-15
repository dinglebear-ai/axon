package com.axon.app.ui.theme

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Test
import tv.tootie.aurora.theme.DarkAuroraExtraColors

class AxonThemeTest {

    /**
     * Visual-parity guard for the derive-from-lib refactor (dab6.7).
     *
     * The four [DarkAuroraExtraColors]-backed dark fields ([AxonPalette.hover],
     * [AxonPalette.warn], [AxonPalette.success], [AxonPalette.accentPink]) MUST equal
     * the canonical [AxonDarkColors] literals. If the aurora lib bumps any of these
     * tokens, this fails — catching a silent color drift at compile-via-test time.
     *
     * The remaining 15 lib-backed fields derive from `MaterialTheme.colorScheme.*`,
     * whose values the lib stores verbatim via `darkColorScheme(...)`; they are
     * documented inline in AxonTheme.kt with their `== #hex` mapping and verified
     * against the lib's dark token JSON. The orange trio is app-specific (no aurora
     * orange family) and intentionally NOT derived.
     */
    @Test
    fun `aurora extra-color-backed dark fields match canonical literals`() {
        assertEquals(AxonDarkColors.hover, DarkAuroraExtraColors.hoverBg)
        assertEquals(AxonDarkColors.warn, DarkAuroraExtraColors.warn)
        assertEquals(AxonDarkColors.success, DarkAuroraExtraColors.success)
        assertEquals(AxonDarkColors.accentPink, DarkAuroraExtraColors.accentPink)
    }

    @Test
    fun `mixSrgb blends color into target by percentage`() {
        val mixed = mixSrgb(Color(0xFF29B6F6), 12, Color(0xFF0C1A24))

        assertEquals(Color(0xFF0F2D3D), mixed)
    }

    @Test
    fun `toneOf returns the orange async operation trio`() {
        val tone = AxonDarkColors.toneOf(AxonTone.Orange)

        assertEquals(Color(0xFFFF9645), tone.base)
        assertEquals(Color(0xFFFFB474), tone.fg)
        assertEquals(Color(0xFFC96A1C), tone.deep)
    }

    @Test
    fun `toneOf collapses to cyan when color coding is disabled`() {
        val tone = AxonDarkColors.toneOf(AxonTone.Rose, colorCode = false)

        assertEquals(Color(0xFF29B6F6), tone.base)
        assertEquals(Color(0xFF67CBFA), tone.fg)
        assertEquals(Color(0xFF1C7FAC), tone.deep)
    }
}
