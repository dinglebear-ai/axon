package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tv.tootie.aurora.components.AuroraThinking

@Composable
fun UserBubble(text: String, modifier: Modifier = Modifier) {
    Box(modifier = modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
        Text(
            text = text,
            modifier = Modifier
                .widthIn(max = 280.dp)
                .background(Color(0x1A29B6F6), RoundedCornerShape(16.dp, 16.dp, 4.dp, 16.dp))
                .border(1.dp, Color(0x4029B6F6), RoundedCornerShape(16.dp, 16.dp, 4.dp, 16.dp))
                .padding(horizontal = 12.dp, vertical = 8.dp),
            fontSize = 13.sp,
            color = Color(0xFFE6F4FB),
            lineHeight = 19.sp,
        )
    }
}

@Composable
fun AxonBubble(
    text: String,
    isStreaming: Boolean = false,
    modifier: Modifier = Modifier,
) {
    Row(modifier = modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(
            modifier = Modifier
                .size(24.dp)
                .background(Color(0xFF0C1A24), RoundedCornerShape(7.dp))
                .border(1.dp, Color(0x4D29B6F6), RoundedCornerShape(7.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Text("✦", fontSize = 11.sp, color = Color(0xFF29B6F6))
        }

        Column(modifier = Modifier.widthIn(max = 280.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                "AXON",
                fontSize = 9.sp,
                fontWeight = FontWeight.Bold,
                color = Color(0xFF29B6F6),
                letterSpacing = 0.8.sp,
            )
            if (isStreaming && text.isEmpty()) {
                AuroraThinking(modifier = Modifier.padding(top = 4.dp))
            } else {
                Text(
                    text = text,
                    modifier = Modifier
                        .background(Color(0xFF102330), RoundedCornerShape(4.dp, 14.dp, 14.dp, 14.dp))
                        .border(1.dp, Color(0xFF1D3D4E), RoundedCornerShape(4.dp, 14.dp, 14.dp, 14.dp))
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                    fontSize = 13.sp,
                    color = Color(0xFFE6F4FB),
                    lineHeight = 19.sp,
                )
            }
        }
    }
}
