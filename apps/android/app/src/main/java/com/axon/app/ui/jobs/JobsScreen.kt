package com.axon.app.ui.jobs

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.WorkOutline
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.axon.app.ui.common.EmptyContent
import tv.tootie.aurora.components.AuroraSeparator

/**
 * Placeholder for the Jobs page (pager position 1). Will surface `/v1/status`
 * (and `/v1/crawl/list`, `/v1/embed/list`, `/v1/extract/list`, `/v1/ingest/list`)
 * once the Kotlin client methods are wired.
 */
@Composable
fun JobsScreen() {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Jobs", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()
        EmptyContent(
            title = "Jobs — not yet wired",
            description = "This page will list crawl, embed, extract, and ingest jobs from /v1/status.",
            icon = Icons.Outlined.WorkOutline,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
