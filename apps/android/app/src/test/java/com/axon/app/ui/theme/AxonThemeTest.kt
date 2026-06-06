package com.axon.app.ui.theme

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Test

class AxonThemeTest {

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
