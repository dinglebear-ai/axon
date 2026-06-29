package com.axon.app.ui.common

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

enum class AxonElevation(val value: Dp) {
    Row(2.dp),
    Card(5.dp),
    Floating(10.dp),
}

fun Modifier.axonElevation(
    shape: Shape = RoundedCornerShape(10.dp),
    elevation: AxonElevation = AxonElevation.Card,
): Modifier = shadow(
    elevation = elevation.value,
    shape = shape,
    clip = false,
)
