package com.axon.app.ui.nav

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

private enum class RailGlyph { History, Jobs, Hub, Sliders, Wrench }

private data class SectionDef(val section: DrawerSection, val icon: RailGlyph, val label: String)

private val TopSections = listOf(
    SectionDef(DrawerSection.Sessions,   RailGlyph.History, "Sess"),
    SectionDef(DrawerSection.Jobs,       RailGlyph.Jobs,    "Jobs"),
    SectionDef(DrawerSection.Knowledge,  RailGlyph.Hub,     "Know"),
    SectionDef(DrawerSection.Management, RailGlyph.Sliders, "Mana"),
)
private val BottomSections = listOf(
    SectionDef(DrawerSection.Setup, RailGlyph.Wrench, "Setu"),
)

@Composable
fun AxonRail(
    activeSection: DrawerSection?,
    onHomeClick: () -> Unit,
    onSectionClick: (DrawerSection) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val dimens = AxonTheme.dimens
    Column(
        modifier = modifier
            .width(dimens.railWidth)
            .fillMaxHeight()
            .background(colors.navBg),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Spacer(Modifier.height(10.dp))
        Box(
            modifier = Modifier
                .size(44.dp)
                .pressScale(onClick = onHomeClick)
                .clip(RoundedCornerShape(dimens.rTile)),
            contentAlignment = Alignment.Center,
        ) {
            AxonMarkGlyph(modifier = Modifier.size(26.dp))
        }
        Spacer(Modifier.height(4.dp))
        TopSections.forEach { def ->
            RailItem(def.icon, def.label, activeSection == def.section) { onSectionClick(def.section) }
        }
        Spacer(Modifier.weight(1f))
        BottomSections.forEach { def ->
            RailItem(def.icon, def.label, activeSection == def.section) { onSectionClick(def.section) }
        }
        Spacer(Modifier.height(10.dp))
    }
}

@Composable
private fun RailItem(icon: RailGlyph, label: String, active: Boolean, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    val dimens = AxonTheme.dimens
    val tint = if (active) colors.accentStrong else colors.textMuted
    Box(
        modifier = Modifier
            .size(dimens.railItemWidth, dimens.railItemHeight)
            .semantics { contentDescription = label }
            .pressScale(onClick = onClick)
            .clip(RoundedCornerShape(dimens.rTile))
            .background(if (active) colors.tint(colors.accentPrimary, 12, colors.navBg) else Color.Transparent),
    ) {
        if (active) {
            Box(
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    .width(3.dp)
                    .height(22.dp)
                    .clip(RoundedCornerShape(topEnd = 2.dp, bottomEnd = 2.dp))
                    .background(colors.accentPrimary),
            )
        }
        Box(
            modifier = Modifier.align(Alignment.Center),
            contentAlignment = Alignment.Center,
        ) {
            RailGlyphIcon(icon = icon, color = tint, modifier = Modifier.size(22.dp))
        }
    }
}

@Composable
private fun RailGlyphIcon(icon: RailGlyph, color: Color, modifier: Modifier = Modifier) {
    Canvas(modifier = modifier) {
        val w = size.width
        val h = size.height
        val s = w.coerceAtMost(h)
        val stroke = s * 0.078f
        fun p(x: Float, y: Float) = Offset(w * x, h * y)
        when (icon) {
            RailGlyph.History -> {
                drawArc(
                    color = color,
                    startAngle = 28f,
                    sweepAngle = 302f,
                    useCenter = false,
                    topLeft = p(0.16f, 0.18f),
                    size = androidx.compose.ui.geometry.Size(w * 0.68f, h * 0.68f),
                    style = Stroke(width = stroke, cap = StrokeCap.Round),
                )
                drawLine(color, p(0.18f, 0.33f), p(0.18f, 0.16f), stroke, StrokeCap.Round)
                drawLine(color, p(0.18f, 0.33f), p(0.35f, 0.33f), stroke, StrokeCap.Round)
                drawLine(color, p(0.50f, 0.34f), p(0.50f, 0.52f), stroke, StrokeCap.Round)
                drawLine(color, p(0.50f, 0.52f), p(0.64f, 0.60f), stroke, StrokeCap.Round)
            }
            RailGlyph.Jobs -> {
                drawArc(
                    color = color,
                    startAngle = -34f,
                    sweepAngle = 292f,
                    useCenter = false,
                    topLeft = p(0.15f, 0.15f),
                    size = androidx.compose.ui.geometry.Size(w * 0.70f, h * 0.70f),
                    style = Stroke(width = stroke, cap = StrokeCap.Round),
                )
                drawLine(color, p(0.78f, 0.18f), p(0.78f, 0.36f), stroke, StrokeCap.Round)
                drawLine(color, p(0.78f, 0.36f), p(0.60f, 0.36f), stroke, StrokeCap.Round)
                drawCircle(color, s * 0.12f, p(0.50f, 0.50f))
            }
            RailGlyph.Hub -> {
                val nodes = listOf(p(0.50f, 0.50f), p(0.50f, 0.18f), p(0.20f, 0.34f), p(0.80f, 0.34f), p(0.31f, 0.82f), p(0.69f, 0.82f))
                nodes.drop(1).forEach { drawLine(color, nodes.first(), it, stroke * 0.78f, StrokeCap.Round) }
                drawCircle(color, s * 0.12f, nodes.first(), style = Stroke(width = stroke))
                nodes.drop(1).forEach { drawCircle(color, s * 0.075f, it, style = Stroke(width = stroke)) }
            }
            RailGlyph.Sliders -> {
                drawLine(color, p(0.17f, 0.30f), p(0.83f, 0.30f), stroke, StrokeCap.Round)
                drawLine(color, p(0.17f, 0.70f), p(0.83f, 0.70f), stroke, StrokeCap.Round)
                drawCircle(color, s * 0.105f, p(0.38f, 0.30f), style = Stroke(width = stroke))
                drawCircle(color, s * 0.105f, p(0.64f, 0.70f), style = Stroke(width = stroke))
            }
            RailGlyph.Wrench -> {
                drawLine(color, p(0.31f, 0.75f), p(0.68f, 0.38f), stroke, StrokeCap.Round)
                drawArc(
                    color = color,
                    startAngle = -140f,
                    sweepAngle = 250f,
                    useCenter = false,
                    topLeft = p(0.48f, 0.12f),
                    size = androidx.compose.ui.geometry.Size(w * 0.36f, h * 0.36f),
                    style = Stroke(width = stroke, cap = StrokeCap.Round),
                )
                drawCircle(color, s * 0.09f, p(0.27f, 0.79f), style = Stroke(width = stroke))
            }
        }
    }
}
