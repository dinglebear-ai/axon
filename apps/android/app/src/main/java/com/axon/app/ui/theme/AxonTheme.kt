package com.axon.app.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.R
import tv.tootie.aurora.theme.AuroraTheme

@Immutable
data class AxonPalette(
    val pageBg: Color,
    val navBg: Color,
    val panelMedium: Color,
    val panelStrong: Color,
    val control: Color,
    val hover: Color,
    val borderDefault: Color,
    val borderStrong: Color,
    val textPrimary: Color,
    val textMuted: Color,
    val accentPrimary: Color,
    val accentStrong: Color,
    val accentDeep: Color,
    val accentPink: Color,
    val accentPinkStrong: Color,
    val accentPinkDeep: Color,
    val orange: Color,
    val orangeStrong: Color,
    val orangeDeep: Color,
    val warn: Color,
    val error: Color,
    val success: Color,
    val isDark: Boolean,
)

val AxonDarkColors = AxonPalette(
    pageBg = Color(0xFF07131C),
    navBg = Color(0xFF07111A),
    panelMedium = Color(0xFF102330),
    panelStrong = Color(0xFF13293A),
    control = Color(0xFF0C1A24),
    hover = Color(0xFF17364B),
    borderDefault = Color(0xFF1D3D4E),
    borderStrong = Color(0xFF24536C),
    textPrimary = Color(0xFFE6F4FB),
    textMuted = Color(0xFFA7BCC9),
    accentPrimary = Color(0xFF29B6F6),
    accentStrong = Color(0xFF67CBFA),
    accentDeep = Color(0xFF1C7FAC),
    accentPink = Color(0xFFF9A8C4),
    accentPinkStrong = Color(0xFFFBC4D6),
    accentPinkDeep = Color(0xFFC46B88),
    orange = Color(0xFFFF9645),
    orangeStrong = Color(0xFFFFB474),
    orangeDeep = Color(0xFFC96A1C),
    warn = Color(0xFFC6A36B),
    error = Color(0xFFC78490),
    success = Color(0xFF7DD3C7),
    isDark = true,
)

val AxonLightColors = AxonPalette(
    pageBg = Color(0xFFF0F6F8),
    navBg = Color(0xFFE4EFF3),
    panelMedium = Color(0xFFFFFFFF),
    panelStrong = Color(0xFFEDF4F7),
    control = Color(0xFFE8F2F5),
    hover = Color(0xFFDCEDF2),
    borderDefault = Color(0xFFC5DAE2),
    borderStrong = Color(0xFF9FBFCC),
    textPrimary = Color(0xFF162126),
    textMuted = Color(0xFF4A6872),
    accentPrimary = Color(0xFF0288D1),
    accentStrong = Color(0xFF0277BD),
    accentDeep = Color(0xFF01579B),
    accentPink = Color(0xFFF9A8C4),
    accentPinkStrong = Color(0xFFFBC4D6),
    accentPinkDeep = Color(0xFFC46B88),
    orange = Color(0xFFE0731A),
    orangeStrong = Color(0xFFC25F10),
    orangeDeep = Color(0xFFA8540E),
    warn = Color(0xFF8A6914),
    error = Color(0xFF9C3545),
    success = Color(0xFF2D7D6E),
    isDark = false,
)

enum class AxonTone { Cyan, Rose, Orange }

@Immutable
data class ToneTrio(val base: Color, val fg: Color, val deep: Color)

fun AxonPalette.toneOf(tone: AxonTone, colorCode: Boolean = true): ToneTrio {
    val resolvedTone = if (colorCode) tone else AxonTone.Cyan
    return when (resolvedTone) {
        AxonTone.Cyan -> ToneTrio(accentPrimary, accentStrong, accentDeep)
        AxonTone.Rose -> ToneTrio(accentPink, accentPinkStrong, accentPinkDeep)
        AxonTone.Orange -> ToneTrio(orange, orangeStrong, orangeDeep)
    }
}

fun mixSrgb(color: Color, pct: Int, into: Color): Color {
    val fraction = pct.coerceIn(0, 100) / 100f
    return Color(
        red = color.red * fraction + into.red * (1f - fraction),
        green = color.green * fraction + into.green * (1f - fraction),
        blue = color.blue * fraction + into.blue * (1f - fraction),
        alpha = color.alpha * fraction + into.alpha * (1f - fraction),
    )
}

fun AxonPalette.tint(color: Color, pct: Int, into: Color = control): Color =
    mixSrgb(color = color, pct = pct, into = into)

@Immutable
data class AxonDimens(
    val railWidth: Dp = 60.dp,
    val railItemWidth: Dp = 46.dp,
    val railItemHeight: Dp = 42.dp,
    val drawerWidth: Dp = 224.dp,
    val fabSize: Dp = 52.dp,
    val opRingRadius: Dp = 118.dp,
    val rTile: Dp = 13.dp,
    val rRow: Dp = 11.dp,
    val rBubble: Dp = 13.dp,
)

val AxonDisplayFont = FontFamily(
    Font(R.font.manrope_regular, FontWeight.Normal),
    Font(R.font.manrope_medium, FontWeight.Medium),
    Font(R.font.manrope_semibold, FontWeight.SemiBold),
    Font(R.font.manrope_bold, FontWeight.Bold),
    Font(R.font.manrope_extrabold, FontWeight.ExtraBold),
)

val AxonBodyFont = FontFamily(
    Font(R.font.inter_regular, FontWeight.Normal),
    Font(R.font.inter_medium, FontWeight.Medium),
    Font(R.font.inter_semibold, FontWeight.SemiBold),
    Font(R.font.inter_bold, FontWeight.Bold),
)

val AxonMonoFont = FontFamily(
    Font(R.font.jetbrains_mono_regular, FontWeight.Normal),
    Font(R.font.jetbrains_mono_medium, FontWeight.Medium),
    Font(R.font.jetbrains_mono_semibold, FontWeight.SemiBold),
)

@Immutable
data class AxonFonts(
    val display: FontFamily,
    val body: FontFamily,
    val mono: FontFamily,
)

private val AxonMaterialTypography = Typography(
    displayLarge = TextStyle(fontFamily = AxonDisplayFont, fontWeight = FontWeight.ExtraBold, fontSize = 57.sp, lineHeight = 64.sp),
    displayMedium = TextStyle(fontFamily = AxonDisplayFont, fontWeight = FontWeight.ExtraBold, fontSize = 45.sp, lineHeight = 52.sp),
    displaySmall = TextStyle(fontFamily = AxonDisplayFont, fontWeight = FontWeight.Bold, fontSize = 36.sp, lineHeight = 44.sp),
    headlineLarge = TextStyle(fontFamily = AxonDisplayFont, fontWeight = FontWeight.Bold, fontSize = 32.sp, lineHeight = 40.sp),
    headlineMedium = TextStyle(fontFamily = AxonDisplayFont, fontWeight = FontWeight.Bold, fontSize = 28.sp, lineHeight = 36.sp),
    headlineSmall = TextStyle(fontFamily = AxonDisplayFont, fontWeight = FontWeight.Bold, fontSize = 24.sp, lineHeight = 32.sp),
    titleLarge = TextStyle(fontFamily = AxonDisplayFont, fontWeight = FontWeight.Bold, fontSize = 22.sp, lineHeight = 28.sp),
    titleMedium = TextStyle(fontFamily = AxonDisplayFont, fontWeight = FontWeight.SemiBold, fontSize = 16.sp, lineHeight = 24.sp),
    titleSmall = TextStyle(fontFamily = AxonDisplayFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, lineHeight = 20.sp),
    bodyLarge = TextStyle(fontFamily = AxonBodyFont, fontWeight = FontWeight.Normal, fontSize = 16.sp, lineHeight = 24.sp),
    bodyMedium = TextStyle(fontFamily = AxonBodyFont, fontWeight = FontWeight.Normal, fontSize = 14.sp, lineHeight = 20.sp),
    bodySmall = TextStyle(fontFamily = AxonBodyFont, fontWeight = FontWeight.Normal, fontSize = 12.sp, lineHeight = 16.sp),
    labelLarge = TextStyle(fontFamily = AxonBodyFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, lineHeight = 20.sp),
    labelMedium = TextStyle(fontFamily = AxonBodyFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp, lineHeight = 16.sp),
    labelSmall = TextStyle(fontFamily = AxonBodyFont, fontWeight = FontWeight.SemiBold, fontSize = 11.sp, lineHeight = 16.sp),
)

private val LocalAxonColors = staticCompositionLocalOf { AxonDarkColors }
private val LocalAxonDimens = staticCompositionLocalOf { AxonDimens() }
private val LocalAxonFonts = staticCompositionLocalOf {
    AxonFonts(
        display = FontFamily.Default,
        body = FontFamily.Default,
        mono = FontFamily.Monospace,
    )
}

object AxonTheme {
    val colors: AxonPalette
        @Composable @ReadOnlyComposable get() = LocalAxonColors.current

    val dimens: AxonDimens
        @Composable @ReadOnlyComposable get() = LocalAxonDimens.current

    val fonts: AxonFonts
        @Composable @ReadOnlyComposable get() = LocalAxonFonts.current
}

@Composable
fun AxonTheme(
    dark: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val colors = if (dark) AxonDarkColors else AxonLightColors
    AuroraTheme(darkTheme = dark) {
        val auroraColors = MaterialTheme.colorScheme
        val auroraShapes = MaterialTheme.shapes
        MaterialTheme(
            colorScheme = auroraColors,
            typography = AxonMaterialTypography,
            shapes = auroraShapes,
        ) {
            CompositionLocalProvider(
                LocalAxonColors provides colors,
                LocalAxonDimens provides AxonDimens(),
                LocalAxonFonts provides AxonFonts(
                    display = AxonDisplayFont,
                    body = AxonBodyFont,
                    mono = AxonMonoFont,
                ),
                content = content,
            )
        }
    }
}
