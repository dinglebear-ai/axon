package com.axon.app.ui.common

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp

/**
 * Tracks which list items have already played their entrance reveal, keyed by
 * the item's stable key (url / job id / session id). Hoist one of these at the
 * list level via [rememberRevealState] and pass it to [revealOnce] on each row.
 *
 * LazyColumn recycles row composables, so a naive per-row entrance replays every
 * time a row scrolls back into view. Gating on the stable key fixes that: once a
 * key has revealed, a recycled re-composition of that row sees `true` and renders
 * at the final state with no animation.
 */
internal class ListRevealState {
    private val revealed = mutableStateMapOf<Any, Boolean>()

    /** True once [markRevealed] has run for [key] — survives row recycling. */
    fun hasRevealed(key: Any): Boolean = revealed[key] == true

    fun markRevealed(key: Any) {
        revealed[key] = true
    }
}

@Composable
internal fun rememberRevealState(): ListRevealState = remember { ListRevealState() }

/**
 * Subtle entrance for a list row: fade-in plus a small upward slide
 * (starts ~[slideDp].dp below, translateY→0, alpha 0→1). Plays once per [key]
 * and never replays on scroll — gated through [state].
 *
 * [index] staggers the initial batch by [staggerMs] per position, capped at
 * [maxStaggerMs] so the last rows don't wait forever. Rows whose entrance has
 * already played (recycled, or appended after first load) just render final.
 */
internal fun Modifier.revealOnce(
    state: ListRevealState,
    key: Any,
    index: Int,
    slideDp: Float = 10f,
    staggerMs: Int = 30,
    maxStaggerMs: Int = 240,
    durationMs: Int = 260,
): Modifier = composed {
    val alreadyRevealed = state.hasRevealed(key)
    val slidePx = with(LocalDensity.current) { slideDp.dp.toPx() }
    // Start hidden only when this key has never revealed; recycled rows start final.
    val progress = remember(key) { Animatable(if (alreadyRevealed) 1f else 0f) }

    LaunchedEffect(key) {
        if (!alreadyRevealed) {
            progress.animateTo(
                targetValue = 1f,
                animationSpec = tween(
                    durationMillis = durationMs,
                    delayMillis = (index * staggerMs).coerceAtMost(maxStaggerMs),
                ),
            )
            state.markRevealed(key)
        }
    }

    graphicsLayer {
        alpha = progress.value
        translationY = (1f - progress.value) * slidePx
    }
}
