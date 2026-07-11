package com.axon.app.feature.memory

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.Description
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

@Composable
internal fun KnowledgeResultRow(
    icon: ImageVector,
    title: String,
    detail: String,
    metric: String,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(8.dp)
    val click = onClick?.let { Modifier.clickable(onClick = it) } ?: Modifier
    Row(
        modifier = modifier
            .clip(shape)
            .background(colors.control.copy(alpha = 0.14f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.24f), shape)
            .then(click)
            .widthIn(min = 0.dp)
            .padding(horizontal = 18.dp, vertical = 19.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Row(
            modifier = Modifier.weight(1f),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            IconTile(icon, size = 36.dp, radius = 9.dp, iconSize = 18.dp)
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(7.dp)) {
                Text(
                    title,
                    color = colors.textPrimary,
                    fontSize = 13.7.sp,
                    lineHeight = 18.sp,
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    detail,
                    color = colors.textMuted,
                    fontSize = 12.2.sp,
                    lineHeight = 16.sp,
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Text(
            metric,
            color = colors.accentStrong,
            fontSize = 12.4.sp,
            lineHeight = 16.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
internal fun KnowledgeSourceRow(
    title: String,
    domain: String,
    source: String,
    chunks: Int,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(8.dp)
    Row(
        modifier = modifier
            .clip(shape)
            .background(colors.control.copy(alpha = 0.13f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.12f), shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        IconTile(icon = Icons.Rounded.Description, size = 44.dp, radius = 12.dp, iconSize = 21.dp)
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(5.dp)) {
            Text(
                title,
                color = colors.textPrimary,
                fontSize = 15.5.sp,
                lineHeight = 20.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    domain,
                    color = colors.accentStrong,
                    fontSize = 12.6.sp,
                    lineHeight = 16.sp,
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                Text(
                    "· $source",
                    color = colors.textMuted,
                    fontSize = 12.sp,
                    lineHeight = 16.sp,
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                chunks.toString(),
                color = colors.accentStrong,
                fontSize = 16.sp,
                lineHeight = 19.sp,
                fontWeight = FontWeight.Bold,
                fontFamily = AxonTheme.fonts.body,
            )
            Text(
                "chunks",
                color = colors.textMuted,
                fontSize = 11.sp,
                lineHeight = 13.sp,
                fontFamily = AxonTheme.fonts.body,
            )
        }
        Icon(
            imageVector = Icons.Rounded.ChevronRight,
            contentDescription = null,
            tint = colors.textMuted.copy(alpha = 0.72f),
            modifier = Modifier.size(18.dp),
        )
    }
}

@Composable
private fun IconTile(icon: ImageVector, size: androidx.compose.ui.unit.Dp = 28.dp, radius: androidx.compose.ui.unit.Dp = 7.dp, iconSize: androidx.compose.ui.unit.Dp = 16.dp) {
    val colors = AxonTheme.colors
    Box(
        modifier = Modifier
            .size(size)
            .clip(RoundedCornerShape(radius))
            .background(colors.tint(colors.accentPrimary, 13, colors.control))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.5f), RoundedCornerShape(radius)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = colors.accentStrong,
            modifier = Modifier.size(iconSize),
        )
    }
}
