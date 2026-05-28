package com.axon.app.ui.common

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.axon.app.ui.theme.AxonColors

/**
 * Shared drawer sub-item row used by Management and Setup drawer sections.
 *
 * [trailing] is an optional slot for badges, chevrons, or other decorations.
 * When [onClick] is non-null and [trailing] is null the caller is responsible
 * for providing trailing content (e.g. a chevron icon).
 */
@Composable
fun DrawerSubItem(
    icon: ImageVector,
    label: String,
    detail: String,
    detailColor: Color = AxonColors.TextMuted,
    onClick: (() -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .let {
                if (onClick != null)
                    it.clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick)
                else it
            }
            // vertical = 14.dp gives ~45dp row height with 17dp icon — meets 48dp with icon padding
            .padding(vertical = 14.dp, horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(
            imageVector = icon,
            contentDescription = label,
            tint = if (onClick != null) AxonColors.AccentPrimary else AxonColors.TextMuted,
            modifier = Modifier.size(17.dp),
        )
        Column(modifier = Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.bodySmall, color = AxonColors.TextLabel)
            Text(
                detail,
                style = MaterialTheme.typography.labelSmall,
                color = detailColor,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
        trailing?.invoke()
    }
}
