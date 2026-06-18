package com.axon.app.ui.jobs

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonTheme

@Composable
internal fun DrillHeader(title: String, detail: String, onBack: () -> Unit) {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(50.dp)
            .clip(RoundedCornerShape(9.dp))
            .background(colors.control.copy(alpha = 0.04f), RoundedCornerShape(9.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.08f), RoundedCornerShape(9.dp))
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Icon(
            Icons.AutoMirrored.Rounded.ArrowBack,
            contentDescription = "Back",
            tint = colors.textMuted,
            modifier = Modifier
                .size(26.dp)
                .clickable(onClick = onBack)
                .padding(6.dp),
        )
        Text(
            title,
            color = colors.textPrimary,
            fontSize = 13.sp,
            lineHeight = 17.4.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = AxonTheme.fonts.display,
            modifier = Modifier.weight(1f),
        )
        Text(detail, color = colors.textMuted.copy(alpha = 0.76f), fontSize = 10.9.sp, lineHeight = 13.8.sp, fontFamily = AxonTheme.fonts.mono)
    }
}
