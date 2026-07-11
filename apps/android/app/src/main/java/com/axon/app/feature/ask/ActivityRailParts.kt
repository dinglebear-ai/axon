package com.axon.app.feature.ask

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.DataObject
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonTheme

@Composable
internal fun ToolKv(label: String, value: String, valueColor: Color) {
    val colors = AxonTheme.colors
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            label,
            color = colors.textMuted.copy(alpha = 0.58f),
            fontSize = 10.5.sp,
            lineHeight = 14.sp,
            fontFamily = AxonTheme.fonts.mono,
            modifier = Modifier.width(48.dp),
        )
        Text(
            value.ifBlank { "—" },
            color = valueColor.copy(alpha = 0.9f),
            fontSize = 11.sp,
            lineHeight = 14.sp,
            fontFamily = AxonTheme.fonts.mono,
            modifier = Modifier.weight(1f, fill = false),
        )
    }
}

internal fun activityIcon(name: String): ImageVector {
    val n = name.lowercase()
    return when {
        n.contains("retriev") -> Icons.Rounded.Storage
        n.contains("search") -> Icons.Rounded.Search
        n.contains("ask") || n.contains("synth") || n.contains("answer") || n.contains("generat") -> Icons.Rounded.AutoAwesome
        else -> Icons.Rounded.DataObject
    }
}
