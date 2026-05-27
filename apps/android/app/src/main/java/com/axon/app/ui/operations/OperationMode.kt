package com.axon.app.ui.operations

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.material.icons.filled.CloudDownload
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Map
import androidx.compose.material.icons.filled.Notes
import androidx.compose.material.icons.filled.Public
import androidx.compose.material.icons.filled.Science
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.TravelExplore
import androidx.compose.ui.graphics.vector.ImageVector

/**
 * Active operation in the Operations page. Persisted across recompositions via the
 * shared [OperationsViewModel]; the FAB icon and the body form both react to the
 * current value.
 */
enum class OperationMode(val label: String, val icon: ImageVector) {
    Ask(       "Ask",       Icons.AutoMirrored.Filled.Chat),
    Summarize( "Summarize", Icons.Filled.Notes),
    Research(  "Research",  Icons.Filled.Science),
    Query(     "Query",     Icons.Filled.Search),
    Scrape(    "Scrape",    Icons.Filled.ContentPaste),
    Crawl(     "Crawl",     Icons.Filled.TravelExplore),
    Ingest(    "Ingest",    Icons.Filled.CloudDownload),
    Search(    "Search",    Icons.Filled.Public),
    Map(       "Map",       Icons.Filled.Map);

    companion object {
        val Default = Ask
    }
}
