package com.axon.app.ui.common

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ChevronRight
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
 * When [onClick] is non-null and [trailing] is null, a chevron is rendered automatically.
 * Pass an explicit [trailing] composable for badges or other custom decorations.
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
    val clickModifier = if (onClick != null) {
        Modifier.clickable(
            interactionSource = remember { MutableInteractionSource() },
            indication = null,
            onClick = onClick,
        )
    } else Modifier

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(clickModifier)
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
        when {
            trailing != null -> trailing()
            onClick != null -> Icon(
                Icons.Rounded.ChevronRight,
                contentDescription = null,
                tint = AxonColors.TextMuted,
                modifier = Modifier.size(14.dp),
            )
        }
    }
}
