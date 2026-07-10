package com.axon.app.ui.operations

import androidx.annotation.Keep
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

private fun openApiRoute(method: String, endpointPath: String): String {
    require(method == "POST")
    require(endpointPath.startsWith("/v1/"))
    return endpointPath
}

/**
 * Active operation in the Operations page. Persisted across recompositions via the
 * shared [OperationsViewModel]; the FAB icon and the body form both react to the
 * current value.
 */
@Keep
enum class OperationMode(
    val label: String,
    val icon: ImageVector,
    /** Server-side REST endpoint this mode targets — encoded explicitly so labels can diverge from paths. */
    val endpointPath: String,
) {
    Ask(       "Ask",       Icons.AutoMirrored.Filled.Chat, openApiRoute("POST", "/v1/ask")),
    Summarize( "Summarize", Icons.AutoMirrored.Filled.Notes, openApiRoute("POST", "/v1/summarize")),
    Research(  "Research",  Icons.Filled.Science,           openApiRoute("POST", "/v1/research")),
    Query(     "Query",     Icons.Filled.Search,            openApiRoute("POST", "/v1/query")),
    // Scrape/Crawl/Ingest all route through the unified source pipeline now —
    // the legacy per-family endpoints hard-404 (see AxonClient.submitSource).
    Scrape(    "Scrape",    Icons.Filled.ContentPaste,      openApiRoute("POST", "/v1/sources")),
    Crawl(     "Crawl",     Icons.Filled.TravelExplore,     openApiRoute("POST", "/v1/sources")),
    Ingest(    "Ingest",    Icons.Filled.CloudDownload,     openApiRoute("POST", "/v1/sources")),
    Search(    "Search",    Icons.Filled.Public,            openApiRoute("POST", "/v1/search")),
    Map(       "Map",       Icons.Filled.Map,               openApiRoute("POST", "/v1/map"));

    companion object {
        val Default = Ask

        fun fromNameOrNull(name: String): OperationMode? = runCatching { valueOf(name) }.getOrNull()
    }
}
