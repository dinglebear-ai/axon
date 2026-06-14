package com.axon.app.ui.ask

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.rounded.AttachFile
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import tv.tootie.aurora.components.AuroraSpinner

@Composable
internal fun AskPromptBar(
    value: String,
    onValueChange: (String) -> Unit,
    onSend: () -> Unit,
    loading: Boolean,
    placeholder: String,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val canSend = value.isNotBlank() && !loading
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(13.dp)

    // Focus reads as the field "warming up": border brightens and the fill
    // deepens together rather than snapping on the keyboard appearing.
    val focusSpec = tween<androidx.compose.ui.graphics.Color>(durationMillis = 200)
    val borderColor by animateColorAsState(
        targetValue = colors.tint(colors.accentPrimary, if (focused) 20 else 6, colors.pageBg),
        animationSpec = focusSpec,
        label = "prompt-border",
    )
    val fillAlpha by animateFloatAsState(
        targetValue = if (focused) 0.16f else 0.10f,
        animationSpec = tween(durationMillis = 200),
        label = "prompt-fill",
    )

    fun triggerSend() {
        if (canSend) onSend()
    }

    Row(
        modifier = modifier
            .height(48.dp)
            .clip(shape)
            .background(colors.panelMedium.copy(alpha = fillAlpha), shape)
            .border(width = 1.dp, color = borderColor, shape = shape)
            .padding(start = 9.dp, top = 4.dp, end = 6.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(
            modifier = Modifier.size(34.dp).clip(RoundedCornerShape(9.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Rounded.AttachFile,
                contentDescription = "Attach file",
                tint = colors.textMuted.copy(alpha = 0.72f),
                modifier = Modifier.size(17.dp),
            )
        }
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            enabled = !loading,
            singleLine = true,
            textStyle = TextStyle(
                color = colors.textPrimary,
                fontSize = 14.2.sp,
                fontFamily = AxonTheme.fonts.body,
            ),
            cursorBrush = SolidColor(colors.accentStrong),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = { triggerSend() }),
            modifier = Modifier
                .weight(1f)
                .onFocusChanged { focused = it.isFocused },
            decorationBox = { inner ->
                Box {
                    if (value.isEmpty()) {
                        Text(
                            placeholder,
                            color = colors.textMuted.copy(alpha = 0.72f),
                            fontSize = 14.2.sp,
                            fontFamily = AxonTheme.fonts.body,
                        )
                    }
                    inner()
                }
            },
        )
        SendButton(canSend = canSend, loading = loading, onSend = ::triggerSend)
    }
}

@Composable
private fun SendButton(canSend: Boolean, loading: Boolean, onSend: () -> Unit) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(10.dp)
    // The send affordance lights up as soon as there's something to send.
    val spec = tween<androidx.compose.ui.graphics.Color>(durationMillis = 180)
    val bg by animateColorAsState(
        targetValue = if (canSend) colors.tint(colors.accentPrimary, 10, colors.control)
        else colors.control.copy(alpha = 0.34f),
        animationSpec = spec,
        label = "send-bg",
    )
    val border by animateColorAsState(
        targetValue = if (canSend) colors.tint(colors.accentPrimary, 24, colors.control)
        else colors.borderDefault.copy(alpha = 0.42f),
        animationSpec = spec,
        label = "send-border",
    )
    val iconTint by animateColorAsState(
        targetValue = if (canSend) colors.accentStrong.copy(alpha = 0.9f)
        else colors.textMuted.copy(alpha = 0.72f),
        animationSpec = spec,
        label = "send-icon",
    )
    Box(
        modifier = Modifier
            .size(36.dp)
            .pressScale(enabled = canSend, onClick = onSend)
            .clip(shape)
            .background(bg, shape)
            .border(1.dp, border, shape),
        contentAlignment = Alignment.Center,
    ) {
        if (loading) {
            AuroraSpinner(contentDescription = "Sending", size = 15.dp)
        } else {
            Icon(
                Icons.AutoMirrored.Filled.Send,
                contentDescription = "Send message",
                tint = iconTint,
                modifier = Modifier.size(17.dp),
            )
        }
    }
}
