package com.axon.app.ui.knowledge

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Storage
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.EmptyContent
import tv.tootie.aurora.components.AuroraSeparator

/**
 * Placeholder for the Knowledge page (pager position 2). Will surface
 * `/v1/suggest`, `/v1/sources`, `/v1/domains`, `/v1/stats` once the Kotlin
 * client methods (sources is already wired) are aggregated here.
 */
@Composable
fun KnowledgeScreen() {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Knowledge", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()
        EmptyContent(
            title = "Knowledge — not yet wired",
            description = "This page will combine /v1/suggest, /v1/sources, /v1/domains, and /v1/stats.",
            icon = Icons.Outlined.Storage,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
