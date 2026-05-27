package com.axon.app.ui.options.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey
import com.axon.app.ui.options.components.HeadersField
import kotlinx.coroutines.launch
import tv.tootie.aurora.components.AuroraSelect
import tv.tootie.aurora.components.AuroraTextField

internal object CrawlFormKeys {
    val MAX_PAGES          = intPreferencesKey("mode_options.crawl.max_pages")
    val MAX_DEPTH          = intPreferencesKey("mode_options.crawl.max_depth")
    val RENDER_MODE        = stringPreferencesKey("mode_options.crawl.render_mode")
    val INCLUDE_SUBDOMAINS = booleanPreferencesKey("mode_options.crawl.include_subdomains")
    val HEADERS            = stringSetPreferencesKey("mode_options.crawl.headers")
    val SKIP_EMBED         = booleanPreferencesKey("mode_options.crawl.skip_embed")
    val COLLECTION         = stringPreferencesKey("mode_options.crawl.collection")
    val WAIT               = booleanPreferencesKey("mode_options.crawl.wait")
    val JSON               = booleanPreferencesKey("mode_options.crawl.json")

    val ALL: List<Preferences.Key<*>> = listOf(
        MAX_PAGES, MAX_DEPTH, RENDER_MODE, INCLUDE_SUBDOMAINS,
        HEADERS, SKIP_EMBED, COLLECTION, WAIT, JSON,
    )
}

private const val DEFAULT_MAX_PAGES = 0
private const val DEFAULT_MAX_DEPTH = 10
private const val DEFAULT_RENDER_MODE = "auto-switch"
private const val DEFAULT_INCLUDE_SUBDOMAINS = false
private const val DEFAULT_SKIP_EMBED = false
private const val DEFAULT_COLLECTION = "axon"
private const val DEFAULT_WAIT = false
private const val DEFAULT_JSON = false

private val RENDER_MODES = listOf("http", "chrome", "auto-switch")

@Composable
fun CrawlOptionsForm() {
    val repo = rememberModeOptionsRepository()
    val scope = rememberCoroutineScope()

    var maxPages by rememberPersistedState(CrawlFormKeys.MAX_PAGES, DEFAULT_MAX_PAGES, repo)
    var maxDepth by rememberPersistedState(CrawlFormKeys.MAX_DEPTH, DEFAULT_MAX_DEPTH, repo)
    var renderMode by rememberPersistedState(CrawlFormKeys.RENDER_MODE, DEFAULT_RENDER_MODE, repo)
    var includeSubdomains by rememberPersistedState(CrawlFormKeys.INCLUDE_SUBDOMAINS, DEFAULT_INCLUDE_SUBDOMAINS, repo)
    var skipEmbed by rememberPersistedState(CrawlFormKeys.SKIP_EMBED, DEFAULT_SKIP_EMBED, repo)
    var collection by rememberPersistedState(CrawlFormKeys.COLLECTION, DEFAULT_COLLECTION, repo)
    var wait by rememberPersistedState(CrawlFormKeys.WAIT, DEFAULT_WAIT, repo)
    var json by rememberPersistedState(CrawlFormKeys.JSON, DEFAULT_JSON, repo)

    // Headers persist as a StringSet (DataStore primitive); UI state is a List<String>.
    var headers by remember { mutableStateOf<List<String>>(emptyList()) }
    LaunchedEffect(Unit) {
        runCatching { headers = repo.readStringSet(CrawlFormKeys.HEADERS)?.toList().orEmpty() }
    }

    ModeOptionsFormScaffold(
        title = "Crawl options",
        description = "Multi-page crawl. `wait` / `json` / `skip-embed` are UI-only flags carried into job submission.",
        resetKeys = CrawlFormKeys.ALL,
        repo = repo,
    ) {
        IntField("Max pages (0 = uncapped)", maxPages) { maxPages = it }
        IntField("Max depth", maxDepth) { maxDepth = it }
        AuroraSelect(
            selectedOption = renderMode,
            onOptionSelected = { renderMode = it },
            options = RENDER_MODES,
            label = "Render mode",
            modifier = Modifier.fillMaxWidth(),
        )
        AuroraTextField(
            value = collection,
            onValueChange = { collection = it },
            label = "Collection",
            modifier = Modifier.fillMaxWidth(),
        )
        SwitchRow("Include subdomains", includeSubdomains) { includeSubdomains = it }
        SwitchRow("Skip embed", skipEmbed) { skipEmbed = it }
        SwitchRow("Wait for completion", wait) { wait = it }
        SwitchRow("JSON output", json) { json = it }
        HeadersField(
            headers = headers,
            onChange = { newList ->
                headers = newList
                scope.launch { repo.write(CrawlFormKeys.HEADERS, newList.toSet()) }
            },
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
