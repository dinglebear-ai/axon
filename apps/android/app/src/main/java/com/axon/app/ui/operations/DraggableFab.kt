package com.axon.app.ui.operations

import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.foundation.layout.offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

/**
 * ExtendedFAB that can be dragged anywhere within its host [BoxScope]. Initial
 * anchor is bottom-end with a 96dp bottom inset so the FAB clears the ~80dp
 * prompt-input row that sits at the bottom of every operation screen. Position
 * is remembered in composition only — it resets when the activity is destroyed.
 *
 * @param onClick   Tap handler. Drag gestures consume their pointer events so
 *                  taps and drags never collide.
 * @param icon      Leading icon slot (active mode icon).
 * @param label     Trailing label slot (active mode label).
 * @param padding   Inset from the bottom-end anchor — also bounds the maximum
 *                  drag offset toward those edges.
 */
@Composable
fun BoxScope.DraggableFab(
    onClick: () -> Unit,
    icon: @Composable () -> Unit,
    label: @Composable () -> Unit,
    padding: PaddingValues = PaddingValues(end = 16.dp, bottom = 96.dp),
) {
    var dragOffset by remember { mutableStateOf(Offset.Zero) }
    var parentSize by remember { mutableStateOf(IntSize.Zero) }
    var fabSize by remember { mutableStateOf(IntSize.Zero) }
    val density = LocalDensity.current

    ExtendedFloatingActionButton(
        onClick = onClick,
        icon = icon,
        text = label,
        modifier = Modifier
            .align(Alignment.BottomEnd)
            // Measures the host BoxScope (matchParentSize via the align modifier's
            // parent reference) by reading the FAB's *available* layout space —
            // when offset is zero the FAB sits at bottom-end; (parent - fab) gives
            // the maximum drag distance in each direction.
            .onPlacedRecordParent { parentSize = it }
            .padding(padding)
            .onSizeChanged { fabSize = it }
            .offset { IntOffset(dragOffset.x.roundToInt(), dragOffset.y.roundToInt()) }
            .pointerInput(Unit) {
                detectDragGestures { change, dragAmount ->
                    change.consume()
                    dragOffset = clampOffset(
                        current = dragOffset + dragAmount,
                        parentSize = parentSize,
                        fabSize = fabSize,
                        density = density,
                        padding = padding,
                    )
                }
            },
    )
}

/** Reads the FAB's *containing layout* size when the FAB is placed, without an extra Box. */
private fun Modifier.onPlacedRecordParent(record: (IntSize) -> Unit): Modifier =
    this.then(
        Modifier.onSizeChanged { /* no-op: we capture parent below */ }
            .layout { measurable, constraints ->
                val placeable = measurable.measure(constraints)
                record(IntSize(constraints.maxWidth, constraints.maxHeight))
                layout(placeable.width, placeable.height) { placeable.place(0, 0) }
            },
    )

private fun clampOffset(
    current: Offset,
    parentSize: IntSize,
    fabSize: IntSize,
    density: Density,
    padding: PaddingValues,
): Offset {
    if (parentSize.width == 0 || fabSize.width == 0) return current
    val endPx = with(density) { padding.calculateRightPadding(LayoutDirection.Ltr).toPx() }
    val bottomPx = with(density) { padding.calculateBottomPadding().toPx() }
    // The FAB's natural anchor sits at (parent.w - fab.w - endPx, parent.h - fab.h - bottomPx).
    // Offset is relative to that anchor; clamp so the FAB stays inside the parent.
    val minX = -(parentSize.width - fabSize.width - endPx)
    val maxX = endPx
    val minY = -(parentSize.height - fabSize.height - bottomPx)
    val maxY = bottomPx
    return Offset(
        x = current.x.coerceIn(minX, maxX),
        y = current.y.coerceIn(minY, maxY),
    )
}
