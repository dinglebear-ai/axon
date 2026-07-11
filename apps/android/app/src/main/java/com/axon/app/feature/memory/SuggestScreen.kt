package com.axon.app.feature.memory

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Lightbulb
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.nav.LocalOpenDocument
import tv.tootie.aurora.components.AuroraItem
import tv.tootie.aurora.components.AuroraPromptInput

@Composable
fun SuggestScreen(vm: KnowledgeViewModel = viewModel()) {
    val state by vm.suggest.collectAsStateWithLifecycle()
    val openDoc = LocalOpenDocument.current
    var focus by rememberSaveable { mutableStateOf("") }

    LaunchedEffect(Unit) { vm.loadSuggest(focus = null) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 8.dp),
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
