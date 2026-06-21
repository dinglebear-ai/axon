package com.axon.app.ui.query

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.rounded.Link
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.QueryHitUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.ui.nav.LocalOpenDocument
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import tv.tootie.aurora.components.AuroraPromptInput
import tv.tootie.aurora.components.AuroraSeparator

@Composable
fun QueryScreen(vm: QueryViewModel = viewModel()) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Crossfade(
            targetState = when (uiState) {
                is QueryUiState.Loading -> 1
                is QueryUiState.Results -> 2
                is QueryUiState.Error -> 3
                else -> 0
            },
            animationSpec = tween(200),
            label = "QueryState",
            modifier = Modifier.weight(1f),
        ) { phase ->
            when (phase) {
                1 -> LoadingContent(label = "Searching vectors…", modifier = Modifier.fillMaxSize())
                2 -> {
                    val s = uiState
                    if (s is QueryUiState.Results) {
                        if (s.hits.isEmpty()) {
                            EmptyContent(
                                title = "No results",
                                description = "No matching documents found. Try a different query.",
                                icon = Icons.Filled.Search,
                                modifier = Modifier.fillMaxSize(),
                            )
                        } else {
                            val reveal = rememberRevealState()
                            LazyColumn(
                                modifier = Modifier.fillMaxSize(),
                                verticalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                itemsIndexed(s.hits, key = { _, h -> "${h.url}#${h.rank}" }) { index, hit ->
                                    QueryHitCard(
                                        hit,
                                        modifier = Modifier
                                            .animateItem()
                                            .revealOnce(reveal, "${hit.url}#${hit.rank}", index),
                                    )
                                }
                            }
                        }
                    }
                }
                3 -> {
                    val s = uiState
                    if (s is QueryUiState.Error) {
                        ErrorContent(message = s.message, modifier = Modifier.fillMaxWidth())
                    }
                }
                else -> EmptyContent(
                    title = "Query your knowledge",
                    description = "Search your indexed knowledge using semantic vector similarity",
                    icon = Icons.Filled.Search,
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }

        AuroraSeparator()
        AuroraPromptInput(
            value = input,
            onValueChange = { input = it },
            onSend = { vm.query(input) },
            placeholder = "Query indexed knowledge…",
            loading = uiState is QueryUiState.Loading,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun QueryHitCard(hit: QueryHitUi, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    val openDocument = LocalOpenDocument.current
    val shape = RoundedCornerShape(10.dp)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(colors.control.copy(alpha = 0.13f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.18f), shape)
            .clickable { openDocument(hit.url) }
            .padding(horizontal = 14.dp, vertical = 13.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(
                Icons.Rounded.Link,
                contentDescription = null,
                tint = colors.accentStrong.copy(alpha = 0.78f),
                modifier = Modifier.size(13.dp),
            )
            Text(
                hit.source,
                color = colors.accentStrong,
                fontSize = 11.8.sp,
                lineHeight = 15.sp,
                fontFamily = AxonTheme.fonts.body,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Text(
                "#${hit.rank}  %.3f".format(hit.score),
                color = colors.textMuted.copy(alpha = 0.68f),
                fontSize = 10.2.sp,
                lineHeight = 13.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
            )
        }
        Text(
            hit.snippet,
            color = colors.textPrimary.copy(alpha = 0.88f),
            fontSize = 12.2.sp,
            lineHeight = 17.sp,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 3,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
