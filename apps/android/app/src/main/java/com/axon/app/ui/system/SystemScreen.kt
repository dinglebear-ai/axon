package com.axon.app.ui.system

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.MonitorHeart
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.EmptyContent
import tv.tootie.aurora.components.AuroraSeparator

/**
 * Placeholder for the System page (pager position 3). Will surface `/v1/debug`,
 * `/v1/doctor`, `/v1/smoke`, `/api/panel/stack`, and `axon config` reads once
 * the Kotlin client methods are wired.
 */
@Composable
fun SystemScreen() {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("System", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()
        EmptyContent(
            title = "System — not yet wired",
            description = "This page will combine Debug, Doctor, Smoke, Stack, and Config.",
            icon = Icons.Outlined.MonitorHeart,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
