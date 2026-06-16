package com.axon.app.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.v2.runComposeUiTest
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import tv.tootie.aurora.theme.AuroraTheme
import tv.tootie.aurora.theme.DarkAuroraExtraColors
import tv.tootie.aurora.theme.LocalAuroraColors

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, sdk = [33])
class AxonThemeTest {

    /**
     * Lib-source guard for the four [DarkAuroraExtraColors]-backed dark fields
     * ([AxonPalette.hover], [AxonPalette.warn], [AxonPalette.success],
     * [AxonPalette.accentPink]). These derive from `LocalAuroraColors.current.*`,
     * whose dark values are [DarkAuroraExtraColors] (a public lib surface). If the
     * aurora lib bumps any of these tokens, this fails — catching a silent color
     * drift at compile-via-test time. (Added dab6.7.)
     */
    @Test
    fun `aurora extra-color-backed dark fields match lib tokens`() {
        assertEquals(AxonDarkColors.hover, DarkAuroraExtraColors.hoverBg)
        assertEquals(AxonDarkColors.warn, DarkAuroraExtraColors.warn)
        assertEquals(AxonDarkColors.success, DarkAuroraExtraColors.success)
        assertEquals(AxonDarkColors.accentPink, DarkAuroraExtraColors.accentPink)
    }

    /**
     * Lib-drift guard for all dark fields derived from Aurora's public theme
     * surfaces. This enters the real [AuroraTheme] composition, reads the live
     * [MaterialTheme.colorScheme] and [LocalAuroraColors], and verifies the derived
     * Axon palette still matches the canonical dark appearance byte-for-byte.
     */
    @OptIn(ExperimentalTestApi::class)
    @Test
    fun `derived dark palette matches live aurora theme`() = runComposeUiTest {
        var derived: AxonPalette? = null

        setContent {
            AuroraTheme(darkTheme = true) {
                derived = auroraDerivedDarkPalette(
                    scheme = MaterialTheme.colorScheme,
                    extra = LocalAuroraColors.current,
                )
            }
        }

        runOnIdle {
            assertEquals(AxonDarkColors, derived)
        }
    }

    /**
     * Byte-for-byte consolidation guard for the derive-from-lib refactor (dab6.7/.8/.9).
     *
     * [AxonDarkColors] is the canonical record of the dark appearance. This test
     * pins all 19 DERIVED dark fields to their exact pre-refactor `Color(0x..)` hex,
     * so any future axon-side edit to [AxonDarkColors] fails loudly instead of
     * silently shifting the UI. The live Aurora-derived path is guarded separately
     * by `derived dark palette matches live aurora theme`.
     *
     * The 19 derived fields are every field EXCEPT the orange trio (app-specific —
     * Aurora has no orange family) and the two theme-invariant app tokens
     * ([AxonPalette.onAccentFg], [AxonPalette.iconMuted], which have no lib equivalent).
     * Those non-derived fields are covered by the orange/tone tests below and by the
     * light/dark invariance of the app-specific literals.
     */
    @Test
    fun `derived dark palette fields equal their canonical hex`() {
        // Surface / panel hierarchy (scheme-backed)
        assertEquals(Color(0xFF07131C), AxonDarkColors.pageBg)
        assertEquals(Color(0xFF07111A), AxonDarkColors.navBg)
        assertEquals(Color(0xFF102330), AxonDarkColors.panelMedium)
        assertEquals(Color(0xFF13293A), AxonDarkColors.panelStrong)
        assertEquals(Color(0xFF0C1A24), AxonDarkColors.control)
        // Hover (extra-color-backed)
        assertEquals(Color(0xFF17364B), AxonDarkColors.hover)
        // Borders (scheme-backed)
        assertEquals(Color(0xFF1D3D4E), AxonDarkColors.borderDefault)
        assertEquals(Color(0xFF24536C), AxonDarkColors.borderStrong)
        // Text (scheme-backed)
        assertEquals(Color(0xFFE6F4FB), AxonDarkColors.textPrimary)
        assertEquals(Color(0xFFA7BCC9), AxonDarkColors.textMuted)
        // Cyan accent family (scheme-backed)
        assertEquals(Color(0xFF29B6F6), AxonDarkColors.accentPrimary)
        assertEquals(Color(0xFF67CBFA), AxonDarkColors.accentStrong)
        assertEquals(Color(0xFF1C7FAC), AxonDarkColors.accentDeep)
        // Pink accent family (accentPink extra-backed; strong/deep scheme-backed)
        assertEquals(Color(0xFFF9A8C4), AxonDarkColors.accentPink)
        assertEquals(Color(0xFFFBC4D6), AxonDarkColors.accentPinkStrong)
        assertEquals(Color(0xFFC46B88), AxonDarkColors.accentPinkDeep)
        // Status (warn/success extra-backed; error scheme-backed)
        assertEquals(Color(0xFFC6A36B), AxonDarkColors.warn)
        assertEquals(Color(0xFFC78490), AxonDarkColors.error)
        assertEquals(Color(0xFF7DD3C7), AxonDarkColors.success)
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
