package com.axon.app.ui.fab

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material3.Icon
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

private sealed interface FabState {
    data object Idle  : FabState
    data object Ring  : FabState
    data class Input(val op: FabOp) : FabState
}

@Composable
fun FabLauncher(
    onOpSubmit: (FabOp, String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var state by remember { mutableStateOf<FabState>(FabState.Idle) }
    var fabCenter by remember { mutableStateOf(IntOffset.Zero) }

    BackHandler(enabled = state !is FabState.Idle) {
        state = FabState.Idle
    }

    BoxWithConstraints(modifier = modifier.fillMaxSize()) {
        val density = LocalDensity.current
        val screenCenter = remember(maxWidth, maxHeight) {
            IntOffset(
                x = with(density) { (maxWidth / 2).roundToPx() },
                y = with(density) { (maxHeight / 2).roundToPx() },
            )
        }

        FabRing(
            visible = state is FabState.Ring,
            fabCenterOffset = if (state is FabState.Ring) screenCenter else fabCenter,
            onOpSelected = { op -> state = FabState.Input(op) },
            onDismiss = { state = FabState.Idle },
        )

        if (state is FabState.Input) {
            val op = (state as FabState.Input).op
            FabOpInputCard(
                op = op,
                onSubmit = { input ->
                    state = FabState.Idle
                    onOpSubmit(op, input)
                },
                onDismiss = { state = FabState.Idle },
            )
        }

        if (state is FabState.Idle) {
            Box(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(bottom = 80.dp, end = 16.dp)
                    .size(42.dp)
                    .onGloballyPositioned { coords ->
                        val pos = coords.positionInWindow()
                        fabCenter = IntOffset(
                            x = (pos.x + coords.size.width / 2).roundToInt(),
                            y = (pos.y + coords.size.height / 2).roundToInt(),
                        )
                    }
                    .background(Color(0xFF13293A), RoundedCornerShape(13.dp))
                    .border(1.dp, Color(0xFF24536C), RoundedCornerShape(13.dp))
                    .clickable(remember { MutableInteractionSource() }, indication = null) {
                        state = FabState.Ring
                    },
                contentAlignment = Alignment.Center,
            ) {
                Icon(Icons.Rounded.Add, contentDescription = "Launch operation", tint = Color(0xFF29B6F6), modifier = Modifier.size(20.dp))
            }
        }
    }
}
