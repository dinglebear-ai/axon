package com.axon.app.ui.operations

import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.FloatingActionButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

/**
 * Compact rounded-square FAB that can be dragged anywhere within its host [BoxScope].
 *
 * Initial anchor is bottom-end with a 96dp bottom inset so the FAB clears the ~80dp
 * prompt-input row that sits at the bottom of every operation screen. Position is
 * remembered in composition only — it resets when the activity is destroyed.
 *
 * Colors are taken straight from the Aurora token palette via [LocalAuroraColors]
 * — bright cyan accent on a deep-navy near-black is what the M3 default
 * `primaryContainer` produced, which read as "almost black" on screen.
 *
 * @param onClick   Tap handler. Drag gestures consume their pointer events so
 *                  taps and drags never collide.
 * @param content   Slot for the active mode's icon (no label — the picker sheet
 *                  carries labels). Keep at 24-28dp for a balanced 56dp tile.
 * @param padding   Inset from the bottom-end anchor — also bounds the maximum
 *                  drag offset toward those edges.
 */
@Composable
fun BoxScope.DraggableFab(
    onClick: () -> Unit,
    content: @Composable () -> Unit,
    padding: PaddingValues = PaddingValues(end = 16.dp, bottom = 96.dp),
) {
    var dragOffset by remember { mutableStateOf(Offset.Zero) }
    var parentSize by remember { mutableStateOf(IntSize.Zero) }
    var fabSize by remember { mutableStateOf(IntSize.Zero) }
    val density = LocalDensity.current
    val layoutDir = LocalLayoutDirection.current

    FloatingActionButton(
        onClick = onClick,
        // Rounded-square tile (20dp radius on the default ~56dp tile). Explicit
        // bright-cyan `primary` container — M3's default FAB falls back to
        // `primaryContainer` which Aurora maps to the darker cyan `accentDeep`,
        // reading as near-black in dark mode.
        shape = RoundedCornerShape(20.dp),
        containerColor = MaterialTheme.colorScheme.primary,
        contentColor = MaterialTheme.colorScheme.onPrimary,
        elevation = FloatingActionButtonDefaults.elevation(
            defaultElevation = 6.dp,
            pressedElevation = 8.dp,
        ),
        modifier = Modifier
            .align(Alignment.BottomEnd)
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
                        layoutDir = layoutDir,
                    )
                }
            },
        content = content,
    )
}

/** Reads the FAB's containing-layout max size without an extra Box. */
private fun Modifier.onPlacedRecordParent(record: (IntSize) -> Unit): Modifier =
    this.layout { measurable, constraints ->
        val placeable = measurable.measure(constraints)
        record(IntSize(constraints.maxWidth, constraints.maxHeight))
        layout(placeable.width, placeable.height) { placeable.place(0, 0) }
    }

private fun clampOffset(
    current: Offset,
    parentSize: IntSize,
    fabSize: IntSize,
    density: Density,
    padding: PaddingValues,
    layoutDir: LayoutDirection,
): Offset {
    if (parentSize.width == 0 || fabSize.width == 0) return current
    val endPx = with(density) { padding.calculateEndPadding(layoutDir).toPx() }
    val bottomPx = with(density) { padding.calculateBottomPadding().toPx() }
    val minX = -(parentSize.width - fabSize.width - endPx)
    val maxX = endPx
    val minY = -(parentSize.height - fabSize.height - bottomPx)
    val maxY = bottomPx
    return Offset(
        x = current.x.coerceIn(minX, maxX),
        y = current.y.coerceIn(minY, maxY),
    )
}
