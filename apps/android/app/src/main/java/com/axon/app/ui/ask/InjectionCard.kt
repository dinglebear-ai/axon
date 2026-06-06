package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.FilterAlt
import androidx.compose.material.icons.rounded.TravelExplore
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.fab.FabOp
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf

@Composable
fun InjectionCard(
    op: FabOp,
    target: String,
    jobId: String? = null,
    pageCount: Int? = null,
    chunkCount: Int? = null,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val chip = colors.toneOf(AxonTone.Cyan)
    val warm = colors.toneOf(AxonTone.Orange)
    val icon = when (op) {
        FabOp.Crawl -> Icons.Rounded.TravelExplore
        FabOp.Extract -> Icons.Rounded.FilterAlt
        else -> Icons.Rounded.Download
    }
    val statusLabel = when {
        jobId != null -> "QUEUED"
        op == FabOp.Ingest -> "INGESTED"
        else -> "CRAWLED"
    }
    val verbPast = when {
        jobId != null -> when (op) {
            FabOp.Crawl -> "queued a crawl for"
            FabOp.Extract -> "queued extraction for"
            else -> "queued ingest for"
        }
        op == FabOp.Ingest -> "ingested"
        op == FabOp.Extract -> "extracted"
        else -> "crawled"
    }
    val indexedWhat = when {
        pageCount != null && chunkCount != null ->
            " and indexed $pageCount docs (${"%,d".format(chunkCount)} chunks) into your knowledge base"
        chunkCount != null -> " and indexed ${"%,d".format(chunkCount)} chunks into your knowledge base"
        else -> if (jobId != null) "" else " into your knowledge base"
    }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(colors.tint(chip.base, 6, colors.control), RoundedCornerShape(14.dp))
            .border(1.dp, colors.tint(chip.base, 22, colors.control), RoundedCornerShape(14.dp))
            .padding(12.dp, 12.dp, 13.dp, 12.dp),
        horizontalArrangement = Arrangement.spacedBy(11.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Box(
            modifier = Modifier
                .size(30.dp)
                .background(colors.tint(warm.base, 14, colors.pageBg), RoundedCornerShape(9.dp))
                .border(1.dp, colors.tint(warm.base, 28, colors.pageBg), RoundedCornerShape(9.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = null, tint = warm.fg, modifier = Modifier.size(15.dp))
        }
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Rounded.Check, contentDescription = null, tint = colors.success, modifier = Modifier.size(12.dp))
                Text(
                    statusLabel,
                    fontSize = 10.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = colors.success,
                    fontFamily = AxonTheme.fonts.mono,
                    letterSpacing = 0.4.sp,
                )
            }
            Text(
                "axon mobile just $verbPast ",
                fontSize = 13.sp,
                lineHeight = 20.sp,
                color = colors.textPrimary,
                fontFamily = AxonTheme.fonts.body,
            )
            Text(
                target,
                fontSize = 12.sp,
                lineHeight = 17.sp,
                fontFamily = AxonTheme.fonts.mono,
                color = chip.fg,
            )
            Text(
                "$indexedWhat${if (indexedWhat.isNotEmpty()) " - " else ""}query · retrieve · ask via MCP or CLI.",
                fontSize = 13.sp,
                lineHeight = 20.sp,
                color = colors.textPrimary,
                fontFamily = AxonTheme.fonts.body,
            )
            if (jobId != null) {
                Text(
                    "job $jobId",
                    fontSize = 10.5.sp,
                    color = colors.textMuted,
                    fontFamily = AxonTheme.fonts.mono,
                )
            }
        }
    }
}
