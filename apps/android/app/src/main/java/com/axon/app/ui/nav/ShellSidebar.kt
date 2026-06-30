package com.axon.app.ui.nav

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

internal data class SidebarItem(
    val label: String,
    val value: String,
    val icon: ImageVector,
)

internal val SidebarSheetWidth = 224.dp

@Composable
internal fun AxonSidebarSheet(
    items: List<SidebarItem>,
    selected: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    Column(
        modifier = modifier
            .width(SidebarSheetWidth)
            .fillMaxHeight()
            .background(colors.panelStrong)
            .border(width = 1.dp, color = colors.borderDefault)
            .padding(horizontal = 14.dp, vertical = 18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(48.dp)
                .padding(horizontal = 5.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            AxonMarkGlyph(Modifier.size(24.dp))
            Text(
                "Axon",
                color = colors.textPrimary,
                fontSize = 16.sp,
                fontWeight = FontWeight.ExtraBold,
                fontFamily = AxonTheme.fonts.display,
            )
        }
        Spacer(Modifier.height(3.dp))
        items.forEach { item ->
            AxonSidebarRow(
                item = item,
                selected = item.value == selected,
                onClick = { onSelect(item.value) },
            )
        }
    }
}

@Composable
private fun AxonSidebarRow(
    item: SidebarItem,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(11.dp)
    // Cross-fade every selection-dependent surface so the active row settles in
    // rather than snapping — keeps the rail feeling physical, not stateful.
    val colorSpec = tween<androidx.compose.ui.graphics.Color>(durationMillis = 220)
    val rowBg by animateColorAsState(
        targetValue = if (selected) colors.tint(colors.accentPrimary, 11, colors.panelStrong)
        else colors.panelStrong.copy(alpha = 0.16f),
        animationSpec = colorSpec,
        label = "row-bg",
    )
    val rowBorder by animateColorAsState(
        targetValue = if (selected) colors.tint(colors.accentPrimary, 28, colors.panelStrong)
        else colors.borderDefault.copy(alpha = 0.08f),
        animationSpec = colorSpec,
        label = "row-border",
    )
    val iconTint by animateColorAsState(
        targetValue = if (selected) colors.accentStrong else colors.textMuted,
        animationSpec = colorSpec,
        label = "row-icon",
    )
    val labelColor by animateColorAsState(
        targetValue = if (selected) colors.textPrimary else colors.textMuted,
        animationSpec = colorSpec,
        label = "row-label",
    )
    // The accent rail grows from a hairline to full height on selection.
    val indicatorHeight by animateDpAsState(
        targetValue = if (selected) 22.dp else 0.dp,
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioLowBouncy,
            stiffness = Spring.StiffnessMediumLow,
        ),
        label = "row-indicator",
    )
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(54.dp)
            .clip(shape)
            .background(rowBg, shape)
            .border(1.dp, rowBorder, shape)
            .semantics(mergeDescendants = true) {
                contentDescription = item.label
                role = Role.Button
                this.selected = selected
            }
            .pressScale(role = Role.Button, onClick = onClick)
            .padding(horizontal = 13.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            modifier = Modifier
                .width(3.dp)
                .height(indicatorHeight)
                .clip(RoundedCornerShape(999.dp))
                .background(colors.accentPrimary),
        )
        Icon(
            imageVector = item.icon,
            contentDescription = null,
            tint = iconTint,
            modifier = Modifier.size(20.dp),
        )
        Text(
            text = item.label,
            color = labelColor,
            fontSize = 14.4.sp,
            lineHeight = 19.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
fun AxonMarkGlyph(modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    Canvas(modifier = modifier) {
        val cx = size.width / 2f
        val nodeRadius = size.minDimension * 0.095f
        val stroke = size.minDimension * 0.055f
        val ys = listOf(
            size.height * 0.26f,
            size.height * 0.42f,
            size.height * 0.58f,
            size.height * 0.74f,
        )
        drawLine(colors.borderStrong, Offset(cx, ys[0] + nodeRadius), Offset(cx, ys[3] - nodeRadius), stroke, StrokeCap.Round)
        drawLine(colors.borderStrong, Offset(cx, ys[0] - nodeRadius * 1.4f), Offset(cx - size.width * 0.24f, 0f), stroke, StrokeCap.Round)
        drawLine(colors.borderStrong, Offset(cx, ys[0] - nodeRadius * 1.4f), Offset(cx, 0f), stroke, StrokeCap.Round)
        drawLine(colors.borderStrong, Offset(cx, ys[0] - nodeRadius * 1.4f), Offset(cx + size.width * 0.24f, 0f), stroke, StrokeCap.Round)
        drawLine(colors.accentStrong, Offset(cx, ys[3] + nodeRadius * 1.4f), Offset(cx - size.width * 0.24f, size.height), stroke, StrokeCap.Round)
        drawLine(colors.accentStrong, Offset(cx, ys[3] + nodeRadius * 1.4f), Offset(cx, size.height), stroke, StrokeCap.Round)
        drawLine(colors.accentStrong, Offset(cx, ys[3] + nodeRadius * 1.4f), Offset(cx + size.width * 0.24f, size.height), stroke, StrokeCap.Round)
        val fills = listOf(colors.borderStrong, colors.accentDeep, colors.accentPrimary, colors.accentStrong)
        ys.forEachIndexed { index, y ->
            drawCircle(fills[index], nodeRadius, Offset(cx, y))
            if (index < 3) {
                drawCircle(colors.accentStrong, nodeRadius * 1.35f, Offset(cx, y), style = Stroke(width = stroke * 0.65f))
            }
        }
    }
}
