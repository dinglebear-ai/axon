package com.axon.app.ui.fab

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ManageSearch
import androidx.compose.material.icons.automirrored.rounded.Notes
import androidx.compose.material.icons.rounded.*
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector

enum class FabOp(
    val label: String,
    val icon: ImageVector,
    val isAsync: Boolean = false,
    val placeholder: String,
) {
    Scrape(
        label       = "Scrape",
        icon        = Icons.Rounded.ContentPaste,
        placeholder = "https://",
    ),
    Research(
        label       = "Research",
        icon        = Icons.Rounded.Science,
        placeholder = "search query…",
    ),
    Extract(
        label       = "Extract",
        icon        = Icons.Rounded.FilterAlt,
        placeholder = "https://",
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
        icon        = Icons.Rounded.Map,
        placeholder = "https://",
    ),
    Retrieve(
        label       = "Retrieve",
        icon        = Icons.Rounded.Archive,
        placeholder = "https://",
    ),
    Summarize(
        label       = "Summarize",
        icon        = Icons.AutoMirrored.Rounded.Notes,
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
        icon        = Icons.Rounded.Download,
        isAsync     = true,
        placeholder = "URL or github/user/repo…",
    ),
}

val syncOpTint  = Color(0xFF29B6F6)
val syncOpBg    = Color(0xFF13293A)
val asyncOpTint = Color(0xFFC6A36B)
val asyncOpBg   = Color(0x12C6A36B)
