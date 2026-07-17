package com.axon.app.ui.searchweb

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
import androidx.compose.material.icons.outlined.Public
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
import com.axon.app.data.repository.SearchWebHitUi
import com.axon.app.data.repository.SearchWebResultUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.ui.nav.LocalOpenDocument
import com.axon.app.ui.theme.AxonTheme
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraPromptInput
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone

/**
 * Search mode screen: live web search via Tavily, results auto-indexed server-side.
 *
 * R16 — when the server rejects enqueueing some result-driven source jobs
 * (`sourceJobsRejected > 0`) AND we still have results to show, surface a Warn
 * callout so the user knows indexing is degraded (queue cap likely hit).
 */
@Composable
fun SearchWebScreen(vm: SearchWebViewModel = viewModel()) {
    val state by vm.uiState.collectAsStateWithLifecycle()
    val openDoc = LocalOpenDocument.current
    var input by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        when (val s = state) {
            Resource.Idle -> EmptyContent(
                title = "Search the live web",
                description = "Results are auto-indexed into your knowledge base.",
                icon = Icons.Outlined.Public,
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
            )
            Resource.Loading -> LoadingContent(
                label = "Searching…",
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
            )
            is Resource.Error -> {
                ErrorContent(message = s.message, modifier = Modifier.fillMaxWidth())
                androidx.compose.foundation.layout.Spacer(Modifier.weight(1f))
            }
            is Resource.Ready -> {
                val result = s.value
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    AuroraStatusIndicator(
                        tone = AuroraStatusTone.Queued,
                        label = "${result.sourceJobsEnqueued} source jobs enqueued",
                    )
                    // R16 — source auto-index backpressure callout.
                    if (result.sourceJobsRejected > 0 && result.results.isNotEmpty()) {
                        AuroraCallout(
                            title = "Auto-index queue full",
                            message = "Some results were not enqueued for indexing — try again later.",
                            variant = AuroraCalloutVariant.Warn,
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                    val reveal = rememberRevealState()
                    LazyColumn(
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        itemsIndexed(result.results, key = { _, it -> it.url }) { index, hit ->
                            SearchWebResultCard(
                                hit = hit,
                                onClick = { openDoc(hit.url) },
                                modifier = Modifier
                                    .animateItem()
                                    .revealOnce(reveal, hit.url, index)
                                    .fillMaxWidth(),
                            )
                        }
                    }
                }
            }
        }

        AuroraSeparator()
        AuroraPromptInput(
            value = input,
            onValueChange = { input = it },
            onSend = {
                vm.submit(input)
                input = ""
            },
            placeholder = "Search the web…",
            loading = state is Resource.Loading,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun SearchWebResultCard(
    hit: SearchWebHitUi,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(10.dp)
    Column(
        modifier = modifier
            .clip(shape)
            .background(colors.control.copy(alpha = 0.13f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.18f), shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 13.dp),
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Text(
            hit.title,
            color = colors.textPrimary,
            fontSize = 13.2.sp,
            lineHeight = 17.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            Icon(
                Icons.Rounded.Link,
                contentDescription = null,
                tint = colors.accentStrong.copy(alpha = 0.7f),
                modifier = Modifier.size(11.dp),
            )
            Text(
                hit.url,
                color = colors.accentStrong.copy(alpha = 0.84f),
                fontSize = 10.8.sp,
                lineHeight = 13.sp,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        hit.snippet?.let { snippet ->
            Text(
                snippet,
                color = colors.textMuted.copy(alpha = 0.78f),
                fontSize = 11.8.sp,
                lineHeight = 16.sp,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
