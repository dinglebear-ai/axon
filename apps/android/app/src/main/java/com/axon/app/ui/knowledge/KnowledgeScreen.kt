package com.axon.app.ui.knowledge

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.BarChart
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.Public
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.AppNoticeBanner
import com.axon.app.ui.common.NoticeTone
import com.axon.app.ui.knowledge.sections.DomainsSection
import com.axon.app.ui.knowledge.sections.SourcesSection
import com.axon.app.ui.knowledge.sections.StatsSection
import com.axon.app.ui.knowledge.sections.SuggestSection
import com.axon.app.ui.common.Resource
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

enum class KnowledgeTab(val title: String) {
    Suggest("Suggest"),
    Sources("Sources"),
    Domains("Domains"),
    Stats("Stats"),
}

/**
 * Knowledge page — four-tab read-only view over /v1/suggest, /v1/sources,
 * /v1/domains, /v1/stats. Tab selection is `rememberSaveable` so config-change
 * (rotation) restores the user's place. Per-section state lives in
 * [KnowledgeViewModel] with R11 30s memoization, so tab-switching is cheap.
 */
@Composable
fun KnowledgeScreen(
    initialTab: KnowledgeTab = KnowledgeTab.Suggest,
    showChrome: Boolean = true,
    onOpenTab: (KnowledgeTab) -> Unit = {},
    onOpenDocument: (String) -> Unit = {},
    vm: KnowledgeViewModel = viewModel(),
) {
    var selected by rememberSaveable(initialTab) { mutableIntStateOf(initialTab.ordinal) }
    val suggest by vm.suggest.collectAsStateWithLifecycle()
    val sources by vm.sources.collectAsStateWithLifecycle()
    val domains by vm.domains.collectAsStateWithLifecycle()
    val stats by vm.stats.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) {
        vm.loadSuggest(focus = null)
        vm.loadSources()
        vm.loadDomains(limit = 200)
        vm.loadStats()
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 6.dp, vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (showChrome) {
            val unavailable = listOf(suggest, sources, domains, stats).count { it is Resource.Error }
            if (unavailable > 0) {
                KnowledgeNotice("$unavailable knowledge ${if (unavailable == 1) "view needs" else "views need"} authentication or a reachable Axon server.")
            }
            KnowledgeMenu(
                selected = selected,
                onSelect = { index ->
                    selected = index
                    onOpenTab(KnowledgeTab.entries[index])
                },
                details = listOf(
                    suggestDetail(suggest),
                    sourcesDetail(sources),
                    domainsDetail(domains),
                    statsDetail(stats),
                ),
            )
        }

        if (!showChrome) {
            when (selected) {
                0 -> SuggestSection(vm)
                1 -> SourcesSection(vm, onOpenDocument = onOpenDocument)
                2 -> DomainsSection(vm)
                3 -> StatsSection(vm)
            }
        }
    }
}

@Composable
private fun KnowledgeMenu(
    selected: Int,
    onSelect: (Int) -> Unit,
    details: List<String>,
) {
    Column(
        verticalArrangement = Arrangement.spacedBy(7.dp),
        modifier = Modifier
            .fillMaxWidth()
            .widthIn(max = 440.dp),
    ) {
        KnowledgeMenuRow(
            icon = Icons.Rounded.AutoAwesome,
            label = "Suggest",
            detail = details.getOrElse(0) { "" },
            selected = selected == 0,
            onClick = { onSelect(0) },
        )
        KnowledgeMenuRow(
            icon = Icons.Rounded.Folder,
            label = "Sources",
            detail = details.getOrElse(1) { "" },
            selected = selected == 1,
            onClick = { onSelect(1) },
        )
        KnowledgeMenuRow(
            icon = Icons.Rounded.Public,
            label = "Domains",
            detail = details.getOrElse(2) { "" },
            selected = selected == 2,
            onClick = { onSelect(2) },
        )
        KnowledgeMenuRow(
            icon = Icons.Rounded.BarChart,
            label = "Stats",
            detail = details.getOrElse(3) { "" },
            selected = selected == 3,
            onClick = { onSelect(3) },
        )
    }
}

@Composable
private fun KnowledgeNotice(message: String) {
    AppNoticeBanner(
        message = message,
        tone = NoticeTone.Warn,
        modifier = Modifier
            .fillMaxWidth()
            .widthIn(max = 440.dp),
    )
}

@Composable
private fun KnowledgeMenuRow(
    icon: ImageVector,
    label: String,
    detail: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(8.dp)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(if (selected) colors.tint(colors.accentPrimary, 4, colors.pageBg) else colors.control.copy(alpha = 0.12f), shape)
            .border(1.dp, if (selected) colors.borderStrong.copy(alpha = 0.22f) else colors.borderDefault.copy(alpha = 0.08f), shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 13.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(
            icon,
            contentDescription = null,
            tint = if (selected) colors.accentStrong.copy(alpha = 0.9f) else colors.textMuted.copy(alpha = 0.76f),
            modifier = Modifier.size(16.dp),
        )
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                label,
                color = colors.textPrimary,
                fontSize = 13.1.sp,
                lineHeight = 16.8.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                detail,
                color = colors.textMuted.copy(alpha = 0.78f),
                fontSize = 10.8.sp,
                lineHeight = 13.8.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Icon(
            Icons.Rounded.ChevronRight,
            contentDescription = null,
            tint = if (selected) colors.accentStrong.copy(alpha = 0.8f) else colors.textMuted.copy(alpha = 0.60f),
            modifier = Modifier.size(15.dp),
        )
    }
}
