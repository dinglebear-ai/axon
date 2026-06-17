package com.axon.app.ui.common

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import kotlinx.collections.immutable.toImmutableList
import tv.tootie.aurora.components.AuroraTabs

@Composable
fun AxonCompactTabs(
    tabs: List<String>,
    selectedIndex: Int,
    onTabSelected: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    AuroraTabs(
        tabs = tabs.toImmutableList(),
        selectedIndex = selectedIndex,
        onTabSelected = onTabSelected,
        modifier = modifier
            .fillMaxWidth(),
        compact = true,
    )
}
