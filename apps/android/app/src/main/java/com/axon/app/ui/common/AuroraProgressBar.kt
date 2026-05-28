package com.axon.app.ui.common

import androidx.compose.animation.core.*
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.*
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

enum class ProgressVariant { Cyan, Success, Error, Warn }
enum class ProgressSize { Sm, Default }

private fun variantColors(v: ProgressVariant): List<Color> = when (v) {
    ProgressVariant.Cyan    -> listOf(Color(0xFF1DA8E6), Color(0xFF4DC8FA), Color(0xFF67CBFA))
    ProgressVariant.Success -> listOf(Color(0xFF3A7A74), Color(0xFF7DD3C7))
    ProgressVariant.Error   -> listOf(Color(0xFF7A3040), Color(0xFFC78490))
    ProgressVariant.Warn    -> listOf(Color(0xFF7A5E2E), Color(0xFFC6A36B))
}

@Composable
fun AuroraProgressBar(
    progress: Float?,
    variant: ProgressVariant = ProgressVariant.Cyan,
    size: ProgressSize = ProgressSize.Default,
    modifier: Modifier = Modifier,
) {
    val trackHeight: Dp = if (size == ProgressSize.Sm) 4.dp else 6.dp
    val shape = RoundedCornerShape(50)
    val colors = variantColors(variant)

    val isIndeterminate = progress == null
    val showShimmer = variant == ProgressVariant.Cyan && (progress == null || (progress > 0f && progress < 1f))

    val indetOffset = if (isIndeterminate) {
        val infiniteTransition = rememberInfiniteTransition(label = "pb-indet")
        val offset by infiniteTransition.animateFloat(
            initialValue = -0.35f,
            targetValue = 1.0f,
            animationSpec = infiniteRepeatable(
                animation = tween(1500, easing = FastOutSlowInEasing),
                repeatMode = RepeatMode.Restart,
            ),
            label = "indet",
        )
        offset
    } else {
        0f
    }

    val shimmerOffset = if (showShimmer) {
        val infiniteTransition = rememberInfiniteTransition(label = "pb-shimmer")
        val offset by infiniteTransition.animateFloat(
            initialValue = -0.5f,
            targetValue = 1.5f,
            animationSpec = infiniteRepeatable(
                animation = tween(2200, easing = LinearEasing),
                repeatMode = RepeatMode.Restart,
            ),
            label = "shimmer",
        )
        offset
    } else {
        0f
    }

    val animatedProgress by animateFloatAsState(
        targetValue = progress ?: 0f,
        animationSpec = tween(600),
        label = "det",
    )

    Box(
        modifier = modifier
            .height(trackHeight)
            .clip(shape)
            .background(Color(0xFF0C1A24))
            .border(1.dp, Color(0xFF1D3D4E), shape),
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            val w = this.size.width
            val h = this.size.height
            val r = CornerRadius(h / 2)

            if (isIndeterminate) {
                val fillW = w * 0.35f
                val x = indetOffset * (w + fillW)
                val brush = Brush.horizontalGradient(colors = colors, startX = x, endX = x + fillW)
                drawRoundRect(brush = brush, topLeft = Offset(x, 0f), size = Size(fillW, h), cornerRadius = r)
            } else {
                val fillW = w * animatedProgress.coerceIn(0f, 1f)
                if (fillW > 0f) {
                    val brush = Brush.horizontalGradient(colors = colors, startX = 0f, endX = fillW)
                    drawRoundRect(brush = brush, size = Size(fillW, h), cornerRadius = r)
                }
            }

            if (showShimmer) {
                val sx = shimmerOffset * w
                val sw = w * 0.3f
                val shimmerBrush = Brush.horizontalGradient(
                    colors = listOf(Color.Transparent, Color.White.copy(alpha = 0.32f), Color.Transparent),
                    startX = sx - sw / 2,
                    endX = sx + sw / 2,
                )
                val shimmerWidth = if (progress != null) w * animatedProgress.coerceIn(0f, 1f) else w
                drawContext.canvas.save()
                drawContext.canvas.clipRect(androidx.compose.ui.geometry.Rect(0f, 0f, shimmerWidth, h))
                drawRect(brush = shimmerBrush, size = Size(w, h))
                drawContext.canvas.restore()
            }
        }
    }
}
