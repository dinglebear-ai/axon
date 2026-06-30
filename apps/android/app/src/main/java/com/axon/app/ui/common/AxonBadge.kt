package com.axon.app.ui.common

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

@Composable
fun AxonBadge(
    text: String,
    tone: Color,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val colors = AxonTheme.colors
    Text(
        text,
        color = colors.tint(tone, 82, colors.textPrimary),
        fontSize = if (compact) 10.2.sp else 10.4.sp,
        lineHeight = 13.sp,
        fontFamily = AxonTheme.fonts.body,
        fontWeight = FontWeight.SemiBold,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(tone, 10, colors.control), RoundedCornerShape(999.dp))
            .border(1.dp, colors.tint(tone, 23, colors.control), RoundedCornerShape(999.dp))
            .padding(horizontal = if (compact) 7.dp else 8.dp, vertical = 3.dp),
    )
}
