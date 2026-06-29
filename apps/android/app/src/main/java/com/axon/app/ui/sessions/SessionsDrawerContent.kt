package com.axon.app.ui.sessions

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.BookmarkAdded
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.History
import androidx.compose.material.icons.rounded.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.local.Session
import com.axon.app.ui.common.AppNoticeBanner
import com.axon.app.ui.common.AxonElevation
import com.axon.app.ui.common.NoticeTone
import com.axon.app.ui.common.axonElevation
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import java.net.URLDecoder
import java.nio.charset.StandardCharsets
import java.util.concurrent.TimeUnit

@Composable
fun SessionsDrawerContent(
    onSelect: (String) -> Unit = {},
    vm: SessionsViewModel = viewModel(),
) {
    val sessions by vm.sessions.collectAsStateWithLifecycle()
    val recentAsks by vm.recentAsks.collectAsStateWithLifecycle()
    val syncError by vm.error.collectAsStateWithLifecycle()
    val reveal = rememberRevealState()

    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        LazyColumn(
            modifier = Modifier
                .fillMaxWidth()
                .widthIn(max = 460.dp)
                .padding(start = 6.dp, top = 10.dp, end = 6.dp),
            verticalArrangement = Arrangement.spacedBy(9.dp),
        ) {
            item {
                NewSessionRow(onClick = { onSelect("new") })
            }
            syncError?.let { message ->
                item {
                    SessionSyncErrorRow(message)
                }
            }
            when {
                sessions.isNotEmpty() -> {
                    itemsIndexed(sessions, key = { _, it -> it.id }) { index, session ->
                        SessionRow(
                            session = session,
                            modifier = Modifier
                                .animateItem()
                                .revealOnce(reveal, session.id, index),
                            onSelect = { onSelect(session.id) },
                            onPin   = { vm.pin(session.id) },
                            onUnpin = { vm.unpin(session.id) },
                            onDelete = { vm.delete(session) },
                        )
                    }
                }
                recentAsks.isNotEmpty() -> {
                    itemsIndexed(recentAsks.take(8), key = { _, it -> "ask-${it.id}" }) { index, ask ->
                        AskHistorySessionRow(
                            ask,
                            modifier = Modifier
                                .animateItem()
                                .revealOnce(reveal, "ask-${ask.id}", index),
                        )
                    }
                }
                else -> {
                    item {
                        EmptySessionsRow()
                    }
                }
            }
        }
    }
}

@Composable
private fun SessionSyncErrorRow(message: String) {
    AppNoticeBanner(
        message = message,
        tone = NoticeTone.Warn,
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun NewSessionRow(onClick: () -> Unit) {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(9.dp))
            .background(colors.control.copy(alpha = 0.025f))
            .border(1.dp, colors.tint(colors.accentPrimary, 18, colors.pageBg), RoundedCornerShape(9.dp))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick)
            .semantics(mergeDescendants = true) {
                contentDescription = "New session"
                role = Role.Button
            }
            .padding(horizontal = 15.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(11.dp),
    ) {
        Icon(Icons.Rounded.Add, contentDescription = null, tint = colors.accentStrong.copy(alpha = 0.82f), modifier = Modifier.size(17.dp))
        Text("New Session", color = colors.textPrimary.copy(alpha = 0.9f), fontSize = 13.sp, fontWeight = FontWeight.SemiBold, fontFamily = AxonTheme.fonts.body)
    }
}

@Composable
private fun AskHistorySessionRow(entry: AskHistoryEntry, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    val title = remember(entry.query) { cleanHistoryText(entry.query) }
    val answer = remember(entry.answer) { cleanHistoryPreview(entry.answer) }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(colors.control.copy(alpha = 0.045f))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.11f), RoundedCornerShape(8.dp))
            .padding(horizontal = 14.dp, vertical = 11.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                title,
                fontSize = 12.8.sp,
                lineHeight = 16.8.sp,
                fontWeight = FontWeight.SemiBold,
                color = colors.textPrimary.copy(alpha = 0.9f),
                fontFamily = AxonTheme.fonts.body,
                modifier = Modifier.weight(1f),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                relativeTime(entry.askedAt),
                fontSize = 10.2.sp,
                lineHeight = 13.4.sp,
                color = colors.textMuted.copy(alpha = 0.68f),
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Text(
            answer,
            fontSize = 11.2.sp,
            lineHeight = 14.8.sp,
            color = colors.textMuted.copy(alpha = 0.68f),
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            "Ask history",
            fontSize = 10.2.sp,
            lineHeight = 13.4.sp,
            color = colors.tint(colors.accentPrimary, 82, colors.textPrimary),
            fontFamily = AxonTheme.fonts.body,
            fontWeight = FontWeight.SemiBold,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .clip(RoundedCornerShape(999.dp))
                .background(colors.tint(colors.accentPrimary, 11, colors.control), RoundedCornerShape(999.dp))
                .border(1.dp, colors.tint(colors.accentPrimary, 24, colors.control), RoundedCornerShape(999.dp))
                .padding(horizontal = 8.dp, vertical = 3.dp),
        )
    }
}

private fun cleanHistoryText(value: String): String =
    runCatching { URLDecoder.decode(value, StandardCharsets.UTF_8.name()) }
        .getOrDefault(value)
        .replace(Regex("\\s+"), " ")
        .trim()

private fun cleanHistoryPreview(value: String): String =
    cleanHistoryText(value)
        .substringBefore("## Sources")
        .substringBefore("## Citation Validation")
        .replace(Regex("\\[S\\d+]"), "")
        .replace(Regex("\\s+"), " ")
        .trim()
        .take(180)
        .trimEnd()

@Composable
private fun EmptySessionsRow() {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(colors.control.copy(alpha = 0.34f))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.55f), RoundedCornerShape(8.dp))
            .padding(horizontal = 14.dp, vertical = 13.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(Icons.Rounded.History, contentDescription = null, tint = colors.textMuted, modifier = Modifier.size(16.dp))
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("No sessions yet", color = colors.textPrimary, fontSize = 11.4.sp, fontWeight = FontWeight.SemiBold, fontFamily = AxonTheme.fonts.body)
            Text("Ask a question to start a live session.", color = colors.textMuted, fontSize = 10.4.sp, fontFamily = AxonTheme.fonts.body, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun SessionRow(
    session: Session,
    modifier: Modifier = Modifier,
    onSelect: () -> Unit,
    onPin: () -> Unit,
    onUnpin: () -> Unit,
    onDelete: () -> Unit,
) {
    val colors = AxonTheme.colors
    var showMenu by remember { mutableStateOf(false) }
    val pinned = session.pinnedAt != null

    Box(modifier = modifier) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .axonElevation(RoundedCornerShape(8.dp), AxonElevation.Row)
                .clip(RoundedCornerShape(8.dp))
                .background(colors.control.copy(alpha = 0.075f))
                .border(1.dp, colors.borderDefault.copy(alpha = 0.14f), RoundedCornerShape(8.dp))
                .combinedClickable(
                    onClick = onSelect,
                    onLongClick = { showMenu = true },
                )
                .semantics(mergeDescendants = true) {
                    contentDescription = "${session.title}, ${session.firstMessagePreview}, ${session.turnCount} turns, ${session.injectedOpCount} operations, ${if (pinned) "pinned, " else ""}${relativeTime(session.updatedAt)}"
                    role = Role.Button
                    customActions = listOf(
                        CustomAccessibilityAction(
                            label = if (!pinned) "Pin session" else "Unpin session",
                        ) {
                            if (!pinned) onPin() else onUnpin()
                            true
                        },
                        CustomAccessibilityAction(label = "Delete session") {
                            onDelete()
                            true
                        },
                    )
                }
            .padding(horizontal = 14.dp, vertical = 11.dp),
            verticalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    session.title,
                    fontSize = 12.8.sp,
                    lineHeight = 16.8.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = colors.textPrimary.copy(alpha = 0.9f),
                    fontFamily = AxonTheme.fonts.body,
                    modifier = Modifier.weight(1f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    relativeTime(session.updatedAt),
                    fontSize = 10.2.sp,
                    lineHeight = 13.4.sp,
                    color = colors.textMuted.copy(alpha = 0.68f),
                    fontFamily = AxonTheme.fonts.mono,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                SessionBadge("Session", colors.accentPrimary)
                if (pinned) SessionBadge("Pinned", colors.accentStrong)
                SessionBadge("${session.turnCount} turns", colors.textMuted)
                if (session.injectedOpCount > 0) SessionBadge("${session.injectedOpCount} ops", colors.accentStrong)
                Box(modifier = Modifier.weight(1f))
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(999.dp))
                        .clickable { showMenu = true }
                        .semantics(mergeDescendants = true) {
                            contentDescription = "Session actions"
                            role = Role.Button
                        }
                        .padding(start = 8.dp, end = 6.dp, top = 4.dp, bottom = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    Text(
                        "Actions",
                        color = colors.textMuted.copy(alpha = 0.82f),
                        fontSize = 10.6.sp,
                        lineHeight = 13.sp,
                        fontFamily = AxonTheme.fonts.body,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Icon(
                        imageVector = Icons.Rounded.MoreVert,
                        contentDescription = null,
                        tint = colors.textMuted.copy(alpha = 0.78f),
                        modifier = Modifier.size(15.dp),
                    )
                }
            }
            Text(
                session.firstMessagePreview,
                fontSize = 11.2.sp,
                lineHeight = 14.8.sp,
                color = colors.textMuted.copy(alpha = 0.72f),
                fontFamily = AxonTheme.fonts.body,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }

        DropdownMenu(expanded = showMenu, onDismissRequest = { showMenu = false }) {
            if (!pinned) {
                DropdownMenuItem(
                    text = { Text("Pin session") },
                    leadingIcon = { Icon(Icons.Rounded.BookmarkAdded, contentDescription = null) },
                    onClick = { showMenu = false; onPin() },
                )
            } else {
                DropdownMenuItem(
                    text = { Text("Unpin session") },
                    leadingIcon = { Icon(Icons.Rounded.BookmarkAdded, contentDescription = null) },
                    onClick = { showMenu = false; onUnpin() },
                )
            }
            DropdownMenuItem(
                text = { Text("Delete session", color = MaterialTheme.colorScheme.error) },
                leadingIcon = { Icon(Icons.Rounded.Delete, contentDescription = null, tint = MaterialTheme.colorScheme.error) },
                onClick = { showMenu = false; onDelete() },
            )
        }
    }
}

@Composable
private fun SessionBadge(text: String, tone: Color) {
    val colors = AxonTheme.colors
    Text(
        text,
        color = colors.tint(tone, 82, colors.textPrimary),
        fontSize = 10.2.sp,
        lineHeight = 13.sp,
        fontFamily = AxonTheme.fonts.body,
        fontWeight = FontWeight.SemiBold,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(tone, 10, colors.control), RoundedCornerShape(999.dp))
            .border(1.dp, colors.tint(tone, 23, colors.control), RoundedCornerShape(999.dp))
            .padding(horizontal = 7.dp, vertical = 3.dp),
    )
}

private fun relativeTime(ts: Long): String {
    val ageMs = (System.currentTimeMillis() - ts).coerceAtLeast(0L)
    val minutes = TimeUnit.MILLISECONDS.toMinutes(ageMs)
    val hours = TimeUnit.MILLISECONDS.toHours(ageMs)
    val days = TimeUnit.MILLISECONDS.toDays(ageMs)
    return when {
        minutes < 1 -> "now"
        minutes < 60 -> "${minutes}m ago"
        hours < 24 -> "${hours}h ago"
        days == 1L -> "yesterday"
        else -> "${days}d ago"
    }
}
