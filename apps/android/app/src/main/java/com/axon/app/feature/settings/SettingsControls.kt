package com.axon.app.feature.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.AppNoticeBanner
import com.axon.app.ui.common.CompactActionButton
import com.axon.app.ui.common.NoticeTone
import com.axon.app.ui.theme.AxonTheme

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
                    .fillMaxWidth()
                    .widthIn(max = 460.dp),
            )
        }
        Row(
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            modifier = Modifier
                .fillMaxWidth()
                .widthIn(max = 460.dp),
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
    val tone = when (kind) {
        SettingsFeedbackKind.Success -> NoticeTone.Success
        SettingsFeedbackKind.Error -> NoticeTone.Error
        SettingsFeedbackKind.Info -> NoticeTone.Info
        SettingsFeedbackKind.Warn -> NoticeTone.Warn
    }
    AppNoticeBanner(message = message, modifier = modifier, tone = tone)
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
