package com.axon.app.ui.knowledge.sections

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Lightbulb
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.data.repository.SuggestHitUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.knowledge.KnowledgeViewModel
import com.axon.app.ui.nav.LocalOpenDocument
import tv.tootie.aurora.components.AuroraItem
import tv.tootie.aurora.components.AuroraPromptInput

/**
 * Suggest tab — optional focus query, lists `/v1/suggest` hits as tappable rows.
 * Tap opens the document in DocumentScreen via [LocalOpenDocument].
 *
 * Focus input is user-initiated, so submits pass `force = true` to bypass the
 * 30s memoization window (otherwise a quick re-query with a new focus would
 * short-circuit on the prior result). Initial tab-enter loads with no focus
 * and benefits from memoization on tab-switches.
 */
@Composable
fun SuggestSection(vm: KnowledgeViewModel) {
    val state by vm.suggest.collectAsStateWithLifecycle()
    val openDoc = LocalOpenDocument.current
    var focus by rememberSaveable { mutableStateOf("") }

    LaunchedEffect(Unit) { vm.loadSuggest(focus = null) }

    Column(
        modifier = Modifier.fillMaxSize().padding(top = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        AuroraPromptInput(
            value = focus,
            onValueChange = { focus = it },
            onSend = { vm.loadSuggest(focus.ifBlank { null }, force = true) },
            placeholder = "Focus (optional) — e.g. \"docs\"",
            modifier = Modifier.fillMaxWidth(),
        )

        when (val s = state) {
            Resource.Idle, Resource.Loading -> LoadingContent(
                label = "Loading suggestions…",
                modifier = Modifier.fillMaxWidth(),
            )
            is Resource.Error -> ErrorContent(
                message = s.message,
                onRetry = { vm.loadSuggest(focus.ifBlank { null }, force = true) },
            )
            is Resource.Ready -> {
                val hits = s.value
                if (hits.isEmpty()) {
                    EmptyContent(
                        title = "No suggestions",
                        description = "Try a focus query or index more sources.",
                        icon = Icons.Outlined.Lightbulb,
                        modifier = Modifier.fillMaxWidth(),
                    )
                } else {
                    LazyColumn(
                        modifier = Modifier.fillMaxWidth(),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        items(hits, key = { it.url }) { hit ->
                            AuroraItem(
                                title = hit.url,
                                description = hit.reason,
                                onClick = { openDoc(hit.url) },
                            )
                        }
                    }
                }
            }
        }
    }
}
