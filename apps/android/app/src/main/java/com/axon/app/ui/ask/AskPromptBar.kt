package com.axon.app.ui.ask

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import tv.tootie.aurora.components.AuroraIconButton
import tv.tootie.aurora.components.AuroraIconButtonSize
import tv.tootie.aurora.components.AuroraIconButtonVariant
import tv.tootie.aurora.components.AuroraPromptInput

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

    fun triggerSend() {
        if (canSend) onSend()
    }

    AuroraPromptInput(
        value = value,
        onValueChange = onValueChange,
        onSend = {
            if (loading) onStop() else triggerSend()
        },
        modifier = modifier,
        placeholder = placeholder,
        enabled = true,
        loading = false,
        hasSendableContent = canSend || loading,
        compact = true,
        maxLines = 6,
        textFieldContentDescription = "Ask prompt",
        sendContentDescription = if (loading) "Stop generating" else "Send message",
        leadingContent = if (attachments.isNotEmpty()) {
            { AttachmentChips(attachments = attachments, onRemove = onRemoveAttachment) }
        } else null,
        inlineLeadingContent = {
            AuroraIconButton(
                onClick = onAttachClick,
                imageVector = Icons.Rounded.AttachFile,
                contentDescription = "Attach files",
                variant = if (attachments.isNotEmpty()) AuroraIconButtonVariant.Tonal else AuroraIconButtonVariant.Standard,
                size = AuroraIconButtonSize.Compact,
            )
        },
        actionLeft = {
            AnimatedVisibility(visible = value.isNotEmpty() && !loading) {
                AuroraIconButton(
                    onClick = { onValueChange("") },
                    imageVector = Icons.Rounded.Close,
                    contentDescription = "Clear prompt",
                    size = AuroraIconButtonSize.Compact,
                )
            }
        },
        trailingContent = {
            ModeMenuButton(
                canSend = canSend,
                loading = loading,
                mode = mode,
                onStop = onStop,
                onModeChange = onModeChange,
            )
        },
    )
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

@Composable
private fun ModeMenuButton(
    canSend: Boolean,
    loading: Boolean,
    mode: ConversationMode,
    onStop: () -> Unit,
    onModeChange: (ConversationMode) -> Unit,
) {
    val colors = AxonTheme.colors
    var menuOpen by remember { mutableStateOf(false) }
    Box {
        AuroraIconButton(
            onClick = { if (!loading) menuOpen = true else onStop() },
            contentDescription = if (loading) "Stop generating" else "${mode.label} mode options",
            size = AuroraIconButtonSize.Compact,
            variant = if (canSend || loading) AuroraIconButtonVariant.Tonal else AuroraIconButtonVariant.Standard,
        ) {
            Crossfade(targetState = loading, label = "mode-stop") { isLoading ->
                Icon(
                    imageVector = if (isLoading) Icons.Rounded.Stop else Icons.Rounded.KeyboardArrowDown,
                    contentDescription = null,
                    tint = if (mode == ConversationMode.Chat) colors.orange else colors.accentStrong,
                    modifier = Modifier.size(18.dp),
                )
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
