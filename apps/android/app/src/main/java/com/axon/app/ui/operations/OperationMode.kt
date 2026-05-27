package com.axon.app.ui.operations

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.material.icons.filled.CloudDownload
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Map
import androidx.compose.material.icons.automirrored.filled.Notes
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
enum class OperationMode(
    val label: String,
    val icon: ImageVector,
    /** Server-side REST endpoint this mode targets — encoded explicitly so labels can diverge from paths. */
    val endpointPath: String,
) {
    Ask(       "Ask",       Icons.AutoMirrored.Filled.Chat, "/v1/ask"),
    Summarize( "Summarize", Icons.AutoMirrored.Filled.Notes, "/v1/summarize"),
    Research(  "Research",  Icons.Filled.Science,           "/v1/research"),
    Query(     "Query",     Icons.Filled.Search,            "/v1/query"),
    Scrape(    "Scrape",    Icons.Filled.ContentPaste,      "/v1/scrape"),
    Crawl(     "Crawl",     Icons.Filled.TravelExplore,     "/v1/crawl"),
    Ingest(    "Ingest",    Icons.Filled.CloudDownload,     "/v1/ingest"),
    Search(    "Search",    Icons.Filled.Public,            "/v1/search"),
    Map(       "Map",       Icons.Filled.Map,               "/v1/map");

    companion object {
        val Default = Ask
    }
}
