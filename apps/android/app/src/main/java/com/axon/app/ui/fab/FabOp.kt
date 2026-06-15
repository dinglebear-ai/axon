package com.axon.app.ui.fab

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ManageSearch
import androidx.compose.material.icons.rounded.*
import androidx.compose.ui.graphics.vector.ImageVector

enum class FabOp(
    val label: String,
    val icon: ImageVector,
    val isAsync: Boolean = false,
    val placeholder: String,
) {
    Scrape(
        label       = "Scrape",
        icon        = Icons.Rounded.Article,
        placeholder = "https://",
    ),
    Research(
        label       = "Research",
        icon        = Icons.Rounded.Psychology,
        placeholder = "search query…",
    ),
    Extract(
        label       = "Extract",
        icon        = Icons.Rounded.DataObject,
        placeholder = "https://",
    ),
    Embed(
        label       = "Embed",
        icon        = Icons.Rounded.DatasetLinked,
        isAsync     = true,
        placeholder = "URL, text, or server path…",
    ),
    Query(
        label       = "Query",
        icon        = Icons.AutoMirrored.Rounded.ManageSearch,
        placeholder = "semantic query…",
    ),
    Search(
        label       = "Search",
        icon        = Icons.Rounded.Public,
        placeholder = "search query…",
    ),
    Map(
        label       = "Map",
        icon        = Icons.Rounded.AccountTree,
        placeholder = "https://",
    ),
    Retrieve(
        label       = "Retrieve",
        icon        = Icons.Rounded.Unarchive,
        placeholder = "https://",
    ),
    Summarize(
        label       = "Summarize",
        icon        = Icons.Rounded.Summarize,
        placeholder = "https://",
    ),
    Crawl(
        label       = "Crawl",
        icon        = Icons.Rounded.TravelExplore,
        isAsync     = true,
        placeholder = "https://",
    ),
    Ingest(
        label       = "Ingest",
        icon        = Icons.Rounded.CloudDownload,
        isAsync     = true,
        placeholder = "URL or github/user/repo…",
    ),
}

val FabRingOps = listOf(
    FabOp.Scrape,
    FabOp.Research,
    FabOp.Extract,
    FabOp.Query,
    FabOp.Search,
    FabOp.Map,
    FabOp.Retrieve,
    FabOp.Summarize,
    FabOp.Crawl,
    FabOp.Ingest,
)
