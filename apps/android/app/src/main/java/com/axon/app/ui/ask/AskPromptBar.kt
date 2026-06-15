package com.axon.app.ui.ask

import android.view.HapticFeedbackConstants
import androidx.compose.animation.AnimatedVisibility
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
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.rounded.AttachFile
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Code
import androidx.compose.material.icons.rounded.DataObject
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.InsertDriveFile
import androidx.compose.material.icons.rounded.KeyboardArrowDown
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
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
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
    attachments: List<PromptAttachment>,
    onAttachClick: () -> Unit,
    onRemoveAttachment: (Int) -> Unit,
    onStop: () -> Unit = {},
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val canSend = value.isNotBlank() && !loading
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(15.dp)

    // A solid raised panel so the input reads as a distinct surface instead of
    // blending into the chat behind it; focus brightens the border.
    val borderColor by animateColorAsState(
        targetValue = colors.tint(colors.accentPrimary, if (focused) 30 else 17, colors.pageBg),
        animationSpec = tween(durationMillis = 200),
        label = "prompt-border",
    )
    val fillAlpha by animateFloatAsState(
        targetValue = if (focused) 0.94f else 0.82f,
        animationSpec = tween(durationMillis = 200),
        label = "prompt-fill",
    )

    fun triggerSend() {
        if (canSend) onSend()
    }

    Column(
        modifier = modifier
            .shadow(elevation = 10.dp, shape = shape)
            .clip(shape)
            .background(colors.panelStrong.copy(alpha = fillAlpha), shape)
            .border(width = 1.dp, color = borderColor, shape = shape),
    ) {
        if (attachments.isNotEmpty()) {
            AttachmentChips(attachments = attachments, onRemove = onRemoveAttachment)
        }
        // Composer layout: the field spans full width on top, the tools sit on a
        // slim toolbar beneath — so multi-line prompts get room to breathe and
        // the mode/attach/clear/send controls don't crowd a single row.
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            enabled = !loading,
            singleLine = false,
            maxLines = 6,
            textStyle = TextStyle(
                color = colors.textPrimary,
                fontSize = 15.sp,
                lineHeight = 20.sp,
                fontFamily = AxonTheme.fonts.body,
            ),
            cursorBrush = SolidColor(colors.accentStrong),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = { triggerSend() }),
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 22.dp)
                .padding(start = 14.dp, end = 14.dp, top = 12.dp)
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
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 8.dp, end = 6.dp, top = 4.dp, bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            ToolbarIconButton(
                icon = Icons.Rounded.AttachFile,
                description = "Attach files",
                tint = if (attachments.isNotEmpty()) colors.accentStrong else colors.textMuted.copy(alpha = 0.7f),
                onClick = onAttachClick,
            )
            Spacer(Modifier.weight(1f))
            AnimatedVisibility(visible = value.isNotEmpty() && !loading) {
                ToolbarIconButton(
                    icon = Icons.Rounded.Close,
                    description = "Clear prompt",
                    tint = colors.textMuted.copy(alpha = 0.7f),
                    onClick = { onValueChange("") },
                )
            }
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
private fun ToolbarIconButton(
    icon: ImageVector,
    description: String,
    tint: Color,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            .size(34.dp)
            .clip(RoundedCornerShape(9.dp))
            .pressScale(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = description, tint = tint, modifier = Modifier.size(18.dp))
    }
}

@Composable
private fun AttachmentChips(attachments: List<PromptAttachment>, onRemove: (Int) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(start = 11.dp, end = 9.dp, top = 9.dp),
        horizontalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        attachments.forEachIndexed { index, attachment ->
            AttachmentChip(attachment = attachment, onRemove = { onRemove(index) })
        }
    }
}

@Composable
private fun AttachmentChip(attachment: PromptAttachment, onRemove: () -> Unit) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(9.dp)
    Row(
        modifier = Modifier
            .clip(shape)
            .background(colors.tint(colors.accentPrimary, 12, colors.control), shape)
            .border(1.dp, colors.tint(colors.accentPrimary, 24, colors.control), shape)
            .padding(start = 8.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(
            attachmentIcon(attachment.name),
            contentDescription = null,
            tint = colors.accentStrong,
            modifier = Modifier.size(15.dp),
        )
        Text(
            attachment.name,
            color = colors.textPrimary,
            fontSize = 12.5.sp,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.widthIn(max = 150.dp),
        )
        val meta = buildString {
            append(formatBytes(attachment.sizeBytes))
            if (attachment.truncated) append(" · trimmed")
        }
        if (meta.isNotBlank()) {
            Text(
                meta,
                color = colors.textMuted.copy(alpha = 0.7f),
                fontSize = 10.5.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
            )
        }
        Box(
            modifier = Modifier.size(22.dp).clip(RoundedCornerShape(7.dp)).pressScale(onClick = onRemove),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Rounded.Close,
                contentDescription = "Remove ${attachment.name}",
                tint = colors.textMuted,
                modifier = Modifier.size(14.dp),
            )
        }
    }
}

private fun attachmentIcon(name: String): ImageVector =
    when (name.substringAfterLast('.', "").lowercase()) {
        "rs", "py", "js", "jsx", "ts", "tsx", "kt", "kts", "go", "java", "c", "cc", "cpp",
        "h", "hpp", "rb", "php", "swift", "sh", "bash", "sql",
        -> Icons.Rounded.Code
        "json", "yaml", "yml", "toml", "xml", "csv", "ini", "env", "conf" -> Icons.Rounded.DataObject
        "md", "markdown", "txt", "rst", "log" -> Icons.Rounded.Description
        else -> Icons.Rounded.InsertDriveFile
    }

/**
 * Tap to send; while a response streams it morphs into a stop control. A
 * long-press opens the Ask/Chat menu, and a corner badge shows the current mode
 * (cyan "A" for Ask, orange "C" for Chat) so it's always visible.
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
    val shape = RoundedCornerShape(11.dp)
    var menuOpen by remember { mutableStateOf(false) }
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val pressScaleValue by animateFloatAsState(
        targetValue = if (pressed) 0.94f else 1f,
        animationSpec = spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessHigh),
        label = "send-scale",
    )

    // Rose fill whenever the button is actionable — ready to send OR streaming.
    // At rest it's a present panel button (not a dim/disabled-looking grey), so
    // the mode caret reads as an accent rather than making it look greyed out.
    val active = canSend || loading
    val spec = tween<Color>(durationMillis = 180)
    val bg by animateColorAsState(
        targetValue = if (active) colors.accentPink.copy(alpha = 0.92f) else colors.panelStrong.copy(alpha = 0.7f),
        animationSpec = spec,
        label = "send-bg",
    )
    val border by animateColorAsState(
        targetValue = if (active) colors.accentPinkStrong.copy(alpha = 0.55f) else colors.borderStrong.copy(alpha = 0.6f),
        animationSpec = spec,
        label = "send-border",
    )
    val iconTint by animateColorAsState(
        targetValue = if (active) Color(0xFF06131C) else colors.textPrimary.copy(alpha = 0.58f),
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
        ModeBadge(
            mode = mode,
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .offset(x = 3.dp, y = 3.dp),
        )
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

/**
 * Corner caret on the send button hinting at the long-press mode menu, tinted by
 * the active conversation mode (cyan for Ask, orange for Chat).
 */
@Composable
private fun ModeBadge(mode: ConversationMode, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    val tone = if (mode == ConversationMode.Chat) colors.orange else colors.accentStrong
    Box(
        modifier = modifier
            .size(16.dp)
            .clip(CircleShape)
            .background(colors.pageBg)
            .border(1.dp, tone, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            Icons.Rounded.KeyboardArrowDown,
            contentDescription = "${mode.label} mode — long-press to change",
            tint = tone,
            modifier = Modifier.size(13.dp),
        )
    }
}
