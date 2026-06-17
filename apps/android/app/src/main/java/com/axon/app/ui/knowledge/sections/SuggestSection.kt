package com.axon.app.ui.knowledge.sections

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Send
import androidx.compose.material.icons.outlined.Lightbulb
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.data.repository.SuggestHitUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.ui.knowledge.KnowledgeResultRow
import com.axon.app.ui.knowledge.KnowledgeViewModel
import com.axon.app.ui.nav.LocalOpenDocument
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
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
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        CompactSuggestInput(
            value = focus,
            onValueChange = { focus = it },
            onSend = { vm.loadSuggest(focus.ifBlank { null }, force = true) },
            placeholder = "Focus (optional) — e.g. \"docs\"",
        )

        when (val s = state) {
            Resource.Idle, Resource.Loading -> LoadingContent(
                label = "Loading suggestions…",
                modifier = Modifier.fillMaxWidth(0.84f).widthIn(max = 350.dp),
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
                    val reveal = rememberRevealState()
                    LazyColumn(
                        modifier = Modifier.fillMaxWidth(0.84f).widthIn(max = 350.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        itemsIndexed(hits, key = { _, it -> it.url }) { index, hit ->
                            KnowledgeResultRow(
                                icon = Icons.Rounded.AutoAwesome,
                                title = hit.url,
                                detail = hit.reason ?: "Suggested source gap",
                                metric = "suggest",
                                onClick = { openDoc(hit.url) },
                                modifier = Modifier
                                    .animateItem()
                                    .revealOnce(reveal, hit.url, index),
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun CompactSuggestInput(
    value: String,
    onValueChange: (String) -> Unit,
    onSend: () -> Unit,
    placeholder: String,
) {
    AuroraPromptInput(
        value = value,
        onValueChange = onValueChange,
        onSend = onSend,
        placeholder = placeholder,
        compact = true,
        maxLines = 1,
        hasSendableContent = true,
        textFieldContentDescription = "Suggestion focus",
        sendContentDescription = "Load suggestions",
        modifier = Modifier
            .fillMaxWidth(0.84f)
            .widthIn(max = 350.dp)
    )
}
