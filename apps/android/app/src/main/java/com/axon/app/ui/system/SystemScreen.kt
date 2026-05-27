package com.axon.app.ui.system

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.MonitorHeart
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.NotYetWiredPage

@Composable
fun SystemScreen() {
    NotYetWiredPage(
        title = "System",
        headline = "System — not yet wired",
        description = "This page will combine Debug, Doctor, Smoke, Stack, and Config.",
        icon = Icons.Outlined.MonitorHeart,
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
    )
}
