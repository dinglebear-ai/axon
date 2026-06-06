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
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.BookmarkAdded
import androidx.compose.material.icons.rounded.BookmarkBorder
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.History
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.local.Session
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

@Composable
fun SessionsDrawerContent(
    onSelect: (String) -> Unit = {},
    vm: SessionsViewModel = viewModel(),
) {
    val sessions by vm.sessions.collectAsStateWithLifecycle()

    LazyColumn(
        modifier = Modifier.fillMaxWidth().padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        item {
            NewSessionRow(onClick = { onSelect("new") })
        }
        if (sessions.isEmpty()) {
            item {
                EmptySessionsRow()
            }
        } else {
            items(sessions, key = { it.id }) { session ->
                SessionRow(
                    session = session,
                    onSelect = { onSelect(session.id) },
                    onPin   = { vm.pin(session.id) },
                    onUnpin = { vm.unpin(session.id) },
                    onDelete = { vm.delete(session) },
                )
            }
        }
    }
}

@Composable
private fun NewSessionRow(onClick: () -> Unit) {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(11.dp))
            .background(colors.tint(colors.accentPrimary, 9, colors.panelStrong))
            .border(1.dp, colors.tint(colors.accentPrimary, 20, Color.Transparent), RoundedCornerShape(11.dp))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        Icon(Icons.Rounded.Add, contentDescription = null, tint = colors.accentStrong, modifier = Modifier.size(16.dp))
        Text("New Session", color = colors.textPrimary, fontSize = 12.5.sp, fontWeight = FontWeight.SemiBold, fontFamily = AxonTheme.fonts.body)
    }
}

@Composable
private fun EmptySessionsRow() {
    val colors = AxonTheme.colors
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(11.dp))
            .padding(horizontal = 10.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Icon(Icons.Rounded.History, contentDescription = null, tint = colors.textMuted, modifier = Modifier.size(16.dp))
        Text("No sessions yet", color = colors.textPrimary, fontSize = 11.5.sp, fontWeight = FontWeight.SemiBold, fontFamily = AxonTheme.fonts.body)
        Text("Ask a question to start a live session.", color = colors.textMuted, fontSize = 10.5.sp, fontFamily = AxonTheme.fonts.body)
    }
}

@Composable
private fun SessionRow(
    session: Session,
    onSelect: () -> Unit,
    onPin: () -> Unit,
    onUnpin: () -> Unit,
    onDelete: () -> Unit,
) {
    var showMenu by remember { mutableStateOf(false) }

    Box {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .combinedClickable(
                    onClick = onSelect,
                    onLongClick = { showMenu = true },
                )
                .padding(horizontal = 14.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(
                imageVector = if (session.pinnedAt != null) Icons.Rounded.BookmarkAdded else Icons.Rounded.BookmarkBorder,
                contentDescription = null,
                tint = if (session.pinnedAt != null) Color(0xFFC6A36B) else Color(0xFF4A6374),
                modifier = Modifier.size(16.dp),
            )
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    session.title,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                    color = Color(0xFFE6F4FB),
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    session.firstMessagePreview,
                    fontSize = 11.sp,
                    color = Color(0xFF4A6374),
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Text(
                "${session.turnCount}t",
                fontSize = 10.sp,
                color = Color(0xFF4A6374),
                fontFamily = AxonTheme.fonts.mono,
            )
        }

        DropdownMenu(expanded = showMenu, onDismissRequest = { showMenu = false }) {
            if (session.pinnedAt == null) {
                DropdownMenuItem(
                    text = { Text("Pin") },
                    leadingIcon = { Icon(Icons.Rounded.BookmarkAdded, contentDescription = null) },
                    onClick = { showMenu = false; onPin() },
                )
            } else {
                DropdownMenuItem(
                    text = { Text("Unpin") },
                    leadingIcon = { Icon(Icons.Rounded.BookmarkBorder, contentDescription = null) },
                    onClick = { showMenu = false; onUnpin() },
                )
            }
            DropdownMenuItem(
                text = { Text("Delete", color = MaterialTheme.colorScheme.error) },
                leadingIcon = { Icon(Icons.Rounded.Delete, contentDescription = null, tint = MaterialTheme.colorScheme.error) },
                onClick = { showMenu = false; onDelete() },
            )
        }
    }
}
