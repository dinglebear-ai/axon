package com.axon.app.ui.jobs

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.WorkOutline
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.NotYetWiredPage

@Composable
fun JobsScreen() {
    NotYetWiredPage(
        title = "Jobs",
        headline = "Jobs — not yet wired",
        description = "This page will list crawl, embed, extract, and ingest jobs from /v1/status.",
        icon = Icons.Outlined.WorkOutline,
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 12.dp),
    )
}
