package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.nav.AxonMarkGlyph
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf
import tv.tootie.aurora.components.AuroraThinking

@Composable
fun UserBubble(text: String, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.End,
        verticalAlignment = Alignment.Top,
    ) {
        Row(
            modifier = Modifier.widthIn(max = 340.dp),
            horizontalArrangement = Arrangement.spacedBy(9.dp),
            verticalAlignment = Alignment.Top,
        ) {
            Column(
                modifier = Modifier.weight(1f, fill = false),
                horizontalAlignment = Alignment.End,
            ) {
                Text(
                    text = text,
                    color = colors.textPrimary,
                    fontSize = 14.sp,
                    lineHeight = 20.sp,
                    fontFamily = AxonTheme.fonts.body,
                    modifier = Modifier
                        .background(colors.control, RoundedCornerShape(topStart = 13.dp, topEnd = 4.dp, bottomStart = 13.dp, bottomEnd = 13.dp))
                        .border(1.dp, colors.borderDefault, RoundedCornerShape(topStart = 13.dp, topEnd = 4.dp, bottomStart = 13.dp, bottomEnd = 13.dp))
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                )
            }
            UserAvatar()
        }
    }
}

@Composable
fun AxonBubble(
    text: String,
    isStreaming: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val sourceMarkers = rememberSourceMarkers(text)
    val displayText = text.replace(Regex("\\s*\\[S\\d+\\]"), "")
    Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(7.dp)) {
        if (isStreaming && text.isEmpty()) {
            Row(
                modifier = Modifier.padding(top = 2.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                AssistantAvatar(pulsing = true)
                AuroraThinking()
            }
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
                AssistantAvatar(pulsing = isStreaming)
                Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(7.dp)) {
                    InlineMarkdownText(displayText)
                    if (sourceMarkers.isNotEmpty()) {
                        FlowRow(
                            horizontalArrangement = Arrangement.spacedBy(7.dp),
                            verticalArrangement = Arrangement.spacedBy(7.dp),
                        ) {
                            sourceMarkers.forEachIndexed { index, marker ->
                                SourceCitationPill(index = index + 1, source = marker)
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun UserAvatar() {
    val colors = AxonTheme.colors
    Box(
        modifier = Modifier
            .size(28.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(Brush.linearGradient(listOf(colors.accentDeep, colors.accentPrimary))),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            "JM",
            color = androidx.compose.ui.graphics.Color(0xFF06131C),
            fontSize = 11.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = AxonTheme.fonts.body,
        )
    }
}

@Composable
private fun AssistantAvatar(pulsing: Boolean) {
    val colors = AxonTheme.colors
    val chip = colors.toneOf(AxonTone.Cyan)
    Box(
        modifier = Modifier
            .size(26.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(colors.tint(chip.base, if (pulsing) 18 else 14, colors.control))
            .border(1.dp, colors.tint(chip.base, 30, colors.control), RoundedCornerShape(8.dp)),
        contentAlignment = Alignment.Center,
    ) {
        AxonMarkGlyph(Modifier.size(17.dp))
    }
}

@Composable
private fun InlineMarkdownText(text: String) {
    val colors = AxonTheme.colors
    val code = colors.toneOf(AxonTone.Rose)
    val parts = remember(text) { text.split('`') }
    Text(
        text = buildAnnotatedString {
            parts.forEachIndexed { index, part ->
                if (index % 2 == 0) {
                    append(part)
                } else {
                    withStyle(
                        SpanStyle(
                            color = code.fg,
                            background = colors.tint(code.base, 12, colors.control),
                            fontFamily = AxonTheme.fonts.mono,
                            fontSize = 13.sp,
                        ),
                    ) {
                        append(part)
                    }
                }
            }
        },
        color = colors.textPrimary,
        fontSize = 14.sp,
        lineHeight = 21.sp,
        fontFamily = AxonTheme.fonts.body,
    )
}

@Composable
private fun rememberSourceMarkers(text: String): List<String> = remember(text) {
    Regex("\\[S\\d+\\]")
        .findAll(text)
        .map { it.value.removeSurrounding("[", "]") }
        .distinct()
        .toList()
}

@Composable
private fun SourceCitationPill(index: Int, source: String) {
    val colors = AxonTheme.colors
    val chip = colors.toneOf(AxonTone.Cyan)
    Row(
        modifier = Modifier
            .background(colors.tint(chip.base, 10, colors.control), RoundedCornerShape(999.dp))
            .border(1.dp, colors.tint(chip.base, 26, colors.control), RoundedCornerShape(999.dp))
            .padding(horizontal = 9.dp, vertical = 3.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("$index", color = chip.fg, fontSize = 10.5.sp, fontWeight = FontWeight.Bold, fontFamily = AxonTheme.fonts.mono)
        Text(source, color = colors.textMuted, fontSize = 10.5.sp, fontFamily = AxonTheme.fonts.mono)
    }
}
