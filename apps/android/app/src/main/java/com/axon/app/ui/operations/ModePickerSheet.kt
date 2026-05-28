package com.axon.app.ui.operations

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraSheet

@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun ModePickerSheet(
    activeMode: OperationMode,
    onSelect: (OperationMode) -> Unit,
    onDismiss: () -> Unit,
) {
    AuroraSheet(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                "Select operation",
                style = MaterialTheme.typography.titleMedium,
            )
            LazyVerticalGrid(
                columns = GridCells.Fixed(3),
                contentPadding = PaddingValues(vertical = 4.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(OperationMode.entries) { mode ->
                    ModeTile(
                        mode = mode,
                        selected = mode == activeMode,
                        onClick = { onSelect(mode) },
                    )
                }
            }
        }
    }
}

@Composable
private fun ModeTile(
    mode: OperationMode,
    selected: Boolean,
    onClick: () -> Unit,
) {
    AuroraCard(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
        variant = if (selected) AuroraCardVariant.Filled else AuroraCardVariant.Outlined,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = 16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Icon(
                imageVector = mode.icon,
                contentDescription = mode.label,
                tint = if (selected) {
                    MaterialTheme.colorScheme.primary
                } else {
                    MaterialTheme.colorScheme.onSurface
                },
            )
            Text(
                mode.label,
                style = MaterialTheme.typography.labelMedium,
            )
        }
    }
}
