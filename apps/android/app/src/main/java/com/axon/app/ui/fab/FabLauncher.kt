package com.axon.app.ui.fab

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material3.Icon
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.theme.AxonTheme
import kotlin.math.roundToInt

private sealed interface FabState {
    data object Idle  : FabState
    data object Ring  : FabState
    data class Input(val op: FabOp) : FabState
}

@Composable
fun FabLauncher(
    onOpSubmit: (FabOp, String) -> Unit,
    onOverlayVisibleChange: (Boolean) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    var state by remember { mutableStateOf<FabState>(FabState.Idle) }
    var fabCenter by remember { mutableStateOf(IntOffset.Zero) }
    val colors = AxonTheme.colors

    BackHandler(enabled = state !is FabState.Idle) {
        state = FabState.Idle
    }

    LaunchedEffect(state) {
        onOverlayVisibleChange(state !is FabState.Idle)
    }

    BoxWithConstraints(modifier = modifier.fillMaxSize()) {
        val density = LocalDensity.current
        val imeVisible = WindowInsets.ime.getBottom(density) > 0
        val ringCenter = remember(maxWidth, maxHeight) {
            with(density) {
                IntOffset(
                    x = (maxWidth / 2).roundToPx(),
                    y = (maxHeight * 0.44f).roundToPx(),
                )
            }
        }

        FabRing(
            visible = state is FabState.Ring,
            fabCenterOffset = if (state is FabState.Ring) ringCenter else fabCenter,
            onOpSelected = { op -> state = FabState.Input(op) },
            onDismiss = { state = FabState.Idle },
        )

        (state as? FabState.Input)?.let { input ->
            FabOpInputCard(
                op = input.op,
                onSubmit = { text ->
                    state = FabState.Idle
                    onOpSubmit(input.op, text)
                },
                onDismiss = { state = FabState.Idle },
            )
        }

        // The + recedes as the ring blooms from its centre, and springs back in
        // when the ring closes — the two motions read as one gesture.
        AnimatedVisibility(
            visible = state is FabState.Idle && !imeVisible,
            enter = fadeIn(tween(durationMillis = 180)) +
                scaleIn(
                    initialScale = 0.6f,
                    animationSpec = spring(
                        dampingRatio = Spring.DampingRatioMediumBouncy,
                        stiffness = Spring.StiffnessMedium,
                    ),
                ),
            exit = fadeOut(tween(durationMillis = 110)) +
                scaleOut(targetScale = 0.6f, animationSpec = tween(durationMillis = 130)),
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(bottom = 88.dp, end = 16.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(46.dp)
                    .onGloballyPositioned { coords ->
                        val pos = coords.positionInWindow()
                        fabCenter = IntOffset(
                            x = (pos.x + coords.size.width / 2).roundToInt(),
                            y = (pos.y + coords.size.height / 2).roundToInt(),
                        )
                    }
                    .background(colors.panelStrong.copy(alpha = 0.76f), RoundedCornerShape(15.dp))
                    .border(1.dp, colors.borderStrong.copy(alpha = 0.74f), RoundedCornerShape(15.dp))
                    .pressScale { state = FabState.Ring },
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    Icons.Rounded.Add,
                    contentDescription = "Launch operation",
                    tint = colors.accentStrong.copy(alpha = 0.88f),
                    modifier = Modifier.size(20.dp),
                )
            }
        }
    }
}
