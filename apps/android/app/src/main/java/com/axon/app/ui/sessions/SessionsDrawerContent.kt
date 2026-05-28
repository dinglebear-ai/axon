package com.axon.app.ui.sessions

import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.local.Session
import com.axon.app.ui.common.EmptyContent

@Composable
fun SessionsDrawerContent(
    onSelect: (String) -> Unit = {},
    vm: SessionsViewModel = viewModel(),
) {
    val sessions by vm.sessions.collectAsStateWithLifecycle()

    if (sessions.isEmpty()) {
        EmptyContent(
            title = "No sessions yet",
            description = "Your Ask conversations will appear here",
            icon = Icons.Rounded.History,
            modifier = Modifier.fillMaxWidth().padding(16.dp),
        )
        return
    }

    LazyColumn(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
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
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    session.firstMessagePreview,
                    fontSize = 11.sp,
                    color = Color(0xFF4A6374),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Text(
                "${session.turnCount}t",
                fontSize = 10.sp,
                color = Color(0xFF4A6374),
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
