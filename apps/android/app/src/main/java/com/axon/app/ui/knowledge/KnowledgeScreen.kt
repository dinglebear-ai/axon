package com.axon.app.ui.knowledge

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Storage
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.NotYetWiredPage

@Composable
fun KnowledgeScreen() {
    NotYetWiredPage(
        title = "Knowledge",
        headline = "Knowledge — not yet wired",
        description = "This page will combine /v1/suggest, /v1/sources, /v1/domains, and /v1/stats.",
        icon = Icons.Outlined.Storage,
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
    )
}
