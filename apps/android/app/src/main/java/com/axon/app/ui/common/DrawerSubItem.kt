package com.axon.app.ui.common

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonColors
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

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
    val colors = AxonTheme.colors
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
            .clip(RoundedCornerShape(11.dp))
            .background(if (onClick != null) Color.Transparent else colors.tint(colors.accentPrimary, 5, colors.panelStrong))
            .border(1.dp, Color.Transparent, RoundedCornerShape(11.dp))
            .then(clickModifier)
            .padding(horizontal = 10.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(
            imageVector = icon,
            contentDescription = label,
            tint = if (onClick != null) colors.textMuted else colors.accentStrong,
            modifier = Modifier.size(16.dp),
        )
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(label, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = colors.textPrimary, fontFamily = AxonTheme.fonts.body)
            Text(
                detail,
                fontSize = 9.5.sp,
                color = detailColor,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
        when {
            trailing != null -> trailing()
            onClick != null -> Icon(
                Icons.Rounded.ChevronRight,
                contentDescription = null,
                tint = colors.textMuted,
                modifier = Modifier.size(14.dp),
            )
        }
    }
}
