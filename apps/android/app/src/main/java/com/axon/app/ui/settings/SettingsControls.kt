package com.axon.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

internal enum class SettingsFeedbackKind { Success, Error, Info, Warn }

@Composable
internal fun SettingsActionDock(
    feedback: Pair<String, SettingsFeedbackKind>?,
    primaryLabel: String,
    primaryEnabled: Boolean,
    onPrimary: () -> Unit,
    secondaryLabel: String,
    secondaryIcon: ImageVector,
    onSecondary: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    Column(
        modifier = modifier
            .background(colors.navBg.copy(alpha = 0.98f))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.10f))
            .navigationBarsPadding()
            .padding(horizontal = 16.dp, vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        feedback?.let { (message, kind) ->
            SettingsFeedbackBanner(
                message = message,
                kind = kind,
                modifier = Modifier
                    .fillMaxWidth(0.96f)
                    .widthIn(max = 420.dp),
            )
        }
        Row(
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            modifier = Modifier
                .fillMaxWidth(0.96f)
                .widthIn(max = 420.dp),
        ) {
            CompactActionButton(
                label = primaryLabel,
                onClick = onPrimary,
                modifier = Modifier.weight(1f),
                enabled = primaryEnabled,
            )
            CompactActionButton(
                label = secondaryLabel,
                onClick = onSecondary,
                modifier = Modifier.weight(1f),
                outlined = true,
                icon = secondaryIcon,
            )
        }
    }
}

@Composable
internal fun SettingsFeedbackBanner(
    message: String,
    kind: SettingsFeedbackKind,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val tone = when (kind) {
        SettingsFeedbackKind.Success -> colors.success
        SettingsFeedbackKind.Error -> colors.error
        SettingsFeedbackKind.Info -> colors.accentStrong
        SettingsFeedbackKind.Warn -> colors.warn
    }
    val icon = when (kind) {
        SettingsFeedbackKind.Success -> Icons.Rounded.Check
        SettingsFeedbackKind.Error, SettingsFeedbackKind.Warn -> Icons.Rounded.WarningAmber
        SettingsFeedbackKind.Info -> Icons.Rounded.Refresh
    }
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(8.dp))
            .background(colors.tint(tone, 7, colors.panelStrong).copy(alpha = 0.99f), RoundedCornerShape(8.dp))
            .border(1.dp, tone.copy(alpha = 0.46f), RoundedCornerShape(8.dp))
            .padding(horizontal = 12.dp, vertical = 9.dp),
        horizontalArrangement = Arrangement.spacedBy(9.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .height(24.dp)
                .widthIn(min = 3.dp, max = 3.dp)
                .background(tone, RoundedCornerShape(99.dp)),
        )
        Box(
            modifier = Modifier
                .size(20.dp)
                .background(tone.copy(alpha = 0.18f), RoundedCornerShape(99.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = null, tint = tone, modifier = Modifier.size(14.dp))
        }
        Text(
            message,
            color = colors.textPrimary,
            fontSize = 12.6.sp,
            lineHeight = 17.2.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 4,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
internal fun CompactSettingField(
    label: String,
    value: String,
    onValueChange: (String) -> Unit,
    visualTransformation: VisualTransformation = VisualTransformation.None,
) {
    val colors = AxonTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(7.dp)) {
        Text(label, color = colors.textMuted.copy(alpha = 0.86f), fontSize = 13.sp, lineHeight = 17.sp, fontFamily = AxonTheme.fonts.body)
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            singleLine = true,
            visualTransformation = visualTransformation,
            textStyle = TextStyle(
                color = colors.textPrimary,
                fontSize = 14.sp,
                lineHeight = 19.sp,
                fontFamily = AxonTheme.fonts.mono,
            ),
            modifier = Modifier
                .fillMaxWidth()
                .height(56.dp)
                .background(colors.panelStrong.copy(alpha = 0.72f), RoundedCornerShape(10.dp))
                .border(1.dp, colors.borderStrong.copy(alpha = 0.42f), RoundedCornerShape(10.dp))
                .padding(horizontal = 14.dp),
            decorationBox = { inner ->
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxSize()) {
                    Box(modifier = Modifier.weight(1f)) {
                        if (value.isBlank()) {
                            Text("unset", color = colors.textMuted, fontSize = 14.sp, fontFamily = AxonTheme.fonts.mono)
                        }
                        inner()
                    }
                }
            },
        )
    }
}

@Composable
internal fun CompactActionButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    outlined: Boolean = false,
    icon: ImageVector? = null,
) {
    val colors = AxonTheme.colors
    val bg = if (outlined) colors.pageBg else colors.accentPrimary
    val fg = if (outlined) colors.textMuted else Color.White
    Row(
        modifier = modifier
            .height(46.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(if (enabled) bg else colors.control, RoundedCornerShape(8.dp))
            .border(1.dp, if (outlined) colors.borderStrong.copy(alpha = 0.42f) else colors.accentPrimary.copy(alpha = 0.86f), RoundedCornerShape(8.dp))
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 14.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        icon?.let {
            Icon(it, contentDescription = null, tint = fg, modifier = Modifier.size(16.dp).padding(end = 6.dp))
        }
        Text(
            label,
            color = fg,
            fontSize = 14.sp,
            lineHeight = 18.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
