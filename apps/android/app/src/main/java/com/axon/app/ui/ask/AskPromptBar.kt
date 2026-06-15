package com.axon.app.ui.ask

import android.view.HapticFeedbackConstants
import androidx.compose.animation.Crossfade
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.rounded.AttachFile
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.draw.scale
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

@Composable
internal fun AskPromptBar(
    value: String,
    onValueChange: (String) -> Unit,
    onSend: () -> Unit,
    loading: Boolean,
    placeholder: String,
    mode: ConversationMode,
    onModeChange: (ConversationMode) -> Unit,
    attachmentName: String?,
    onAttachClick: () -> Unit,
    onRemoveAttachment: () -> Unit,
    onStop: () -> Unit = {},
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val canSend = value.isNotBlank() && !loading
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(13.dp)

    // Focus reads as the field "warming up": border brightens and the fill
    // deepens together rather than snapping on the keyboard appearing.
    val borderColor by animateColorAsState(
        targetValue = colors.tint(colors.accentPrimary, if (focused) 20 else 6, colors.pageBg),
        animationSpec = tween(durationMillis = 200),
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

    Column(
        modifier = modifier
            .clip(shape)
            .background(colors.panelMedium.copy(alpha = fillAlpha), shape)
            .border(width = 1.dp, color = borderColor, shape = shape),
    ) {
        if (attachmentName != null) {
            AttachmentChip(name = attachmentName, onRemove = onRemoveAttachment)
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(48.dp)
                .padding(start = 11.dp, top = 4.dp, end = 6.dp, bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(34.dp)
                    .clip(RoundedCornerShape(9.dp))
                    .pressScale(onClick = onAttachClick),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    Icons.Rounded.AttachFile,
                    contentDescription = "Attach file",
                    tint = if (attachmentName != null) colors.accentStrong else colors.textMuted.copy(alpha = 0.66f),
                    modifier = Modifier.size(18.dp),
                )
            }
            BasicTextField(
            value = value,
            onValueChange = onValueChange,
            enabled = !loading,
            singleLine = true,
            textStyle = TextStyle(
                color = colors.textPrimary,
                fontSize = 15.sp,
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
                            fontSize = 15.sp,
                            fontFamily = AxonTheme.fonts.body,
                        )
                    }
                    inner()
                }
            },
        )
            SendButton(
                canSend = canSend,
                loading = loading,
                mode = mode,
                onSend = ::triggerSend,
                onStop = onStop,
                onModeChange = onModeChange,
            )
        }
    }
}

@Composable
private fun AttachmentChip(name: String, onRemove: () -> Unit) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(9.dp)
    Row(modifier = Modifier.fillMaxWidth().padding(start = 11.dp, end = 9.dp, top = 9.dp)) {
        Row(
            modifier = Modifier
                .clip(shape)
                .background(colors.tint(colors.accentPrimary, 12, colors.control), shape)
                .border(1.dp, colors.tint(colors.accentPrimary, 24, colors.control), shape)
                .padding(start = 9.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            Icon(
                Icons.Rounded.AttachFile,
                contentDescription = null,
                tint = colors.accentStrong,
                modifier = Modifier.size(14.dp),
            )
            Text(
                name,
                color = colors.textPrimary,
                fontSize = 12.5.sp,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.widthIn(max = 210.dp),
            )
            Box(
                modifier = Modifier.size(22.dp).clip(RoundedCornerShape(7.dp)).pressScale(onClick = onRemove),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    Icons.Rounded.Close,
                    contentDescription = "Remove attachment",
                    tint = colors.textMuted,
                    modifier = Modifier.size(14.dp),
                )
            }
        }
    }
}

/**
 * Tap to send; long-press to pick the conversation mode (Ask / Chat). The mode
 * toggle used to be tabs at the top of the screen — it now hides behind a
 * long-press here so the single-line prompt stays uncluttered.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SendButton(
    canSend: Boolean,
    loading: Boolean,
    mode: ConversationMode,
    onSend: () -> Unit,
    onStop: () -> Unit,
    onModeChange: (ConversationMode) -> Unit,
) {
    val colors = AxonTheme.colors
    val view = LocalView.current
    val shape = RoundedCornerShape(10.dp)
    var menuOpen by remember { mutableStateOf(false) }
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val pressScaleValue by animateFloatAsState(
        targetValue = if (pressed) 0.94f else 1f,
        animationSpec = spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessHigh),
        label = "send-scale",
    )

    // Rose fill whenever the button is actionable — ready to send OR streaming
    // (so the stop affordance is just as prominent as send).
    val active = canSend || loading
    val spec = tween<androidx.compose.ui.graphics.Color>(durationMillis = 180)
    val bg by animateColorAsState(
        targetValue = if (active) colors.accentPink.copy(alpha = 0.92f)
        else colors.control.copy(alpha = 0.34f),
        animationSpec = spec,
        label = "send-bg",
    )
    val border by animateColorAsState(
        targetValue = if (active) colors.accentPinkStrong.copy(alpha = 0.55f)
        else colors.borderDefault.copy(alpha = 0.42f),
        animationSpec = spec,
        label = "send-border",
    )
    val iconTint by animateColorAsState(
        targetValue = if (active) androidx.compose.ui.graphics.Color(0xFF06131C)
        else colors.textMuted.copy(alpha = 0.72f),
        animationSpec = spec,
        label = "send-icon",
    )

    Box {
        Box(
            modifier = Modifier
                .size(38.dp)
                .scale(pressScaleValue)
                .clip(shape)
                .background(bg, shape)
                .border(1.dp, border, shape)
                .combinedClickable(
                    interactionSource = interaction,
                    indication = null,
                    onClick = {
                        when {
                            loading -> {
                                view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                                onStop()
                            }
                            canSend -> {
                                view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                                onSend()
                            }
                        }
                    },
                    onLongClick = {
                        if (!loading) {
                            view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                            menuOpen = true
                        }
                    },
                ),
            contentAlignment = Alignment.Center,
        ) {
            // Crossfade send <-> stop so cancelling reads as a deliberate state swap.
            Crossfade(targetState = loading, label = "send-stop") { isLoading ->
                if (isLoading) {
                    Icon(
                        Icons.Rounded.Stop,
                        contentDescription = "Stop generating",
                        tint = iconTint,
                        modifier = Modifier.size(18.dp),
                    )
                } else {
                    Icon(
                        Icons.AutoMirrored.Filled.Send,
                        contentDescription = "Send message — long-press to choose Ask or Chat",
                        tint = iconTint,
                        modifier = Modifier.size(18.dp),
                    )
                }
            }
        }
        DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            ConversationMode.entries.forEach { item ->
                val active = item == mode
                DropdownMenuItem(
                    text = {
                        Text(
                            item.label,
                            fontFamily = AxonTheme.fonts.body,
                            color = if (active) colors.accentStrong else colors.textPrimary,
                        )
                    },
                    onClick = {
                        onModeChange(item)
                        menuOpen = false
                    },
                    trailingIcon = {
                        if (active) {
                            Icon(
                                Icons.Rounded.Check,
                                contentDescription = null,
                                tint = colors.accentStrong,
                                modifier = Modifier.size(16.dp),
                            )
                        }
                    },
                )
            }
        }
    }
}
