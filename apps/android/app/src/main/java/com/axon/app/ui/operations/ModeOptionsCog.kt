package com.axon.app.ui.operations

import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.nav.LocalModeOptionsCog

/**
 * Returns an `actionLeft` slot to feed into `AuroraPromptInput` when the
 * current screen is hosted under `OperationsScreen`. Reads
 * [LocalModeOptionsCog]; when no handler is provided (e.g. the screen is
 * being reused outside the Operations host), returns `null` so the input
 * renders without a cog.
 */
@Composable
fun modeOptionsCog(): (@Composable () -> Unit)? {
    val onClick = LocalModeOptionsCog.current ?: return null
    return {
        IconButton(
            onClick = onClick,
            modifier = Modifier.size(36.dp),
        ) {
            Icon(
                imageVector = Icons.Outlined.Tune,
                contentDescription = "Mode options",
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
