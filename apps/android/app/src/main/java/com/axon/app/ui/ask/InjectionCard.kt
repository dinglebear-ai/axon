package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.TravelExplore
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.fab.FabOp

@Composable
fun InjectionCard(
    op: FabOp,
    target: String,
    pageCount: Int? = null,
    chunkCount: Int? = null,
    modifier: Modifier = Modifier,
) {
    val icon = if (op == FabOp.Crawl) Icons.Rounded.TravelExplore else Icons.Rounded.Download
    val verbPast = if (op == FabOp.Crawl) "crawled" else "ingested"
    val indexedWhat = when {
        pageCount != null && chunkCount != null ->
            "and indexed $pageCount docs (${"%,d".format(chunkCount)} chunks)"
        chunkCount != null -> "and indexed ${"%,d".format(chunkCount)} chunks"
        else -> ""
    }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(Color(0x0D29B6F6), RoundedCornerShape(12.dp))
            .border(1.dp, Color(0x2E29B6F6), RoundedCornerShape(12.dp))
            .padding(10.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = Color(0xFFC6A36B),
            modifier = Modifier.size(14.dp).padding(top = 1.dp),
        )
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                "axon mobile just $verbPast",
                fontSize = 10.sp,
                color = Color(0xFFA7BCC9),
            )
            Text(
                target,
                fontSize = 10.sp,
                fontFamily = FontFamily.Monospace,
                color = Color(0xFF67CBFA),
            )
            if (indexedWhat.isNotEmpty()) {
                Text(
                    "$indexedWhat into your knowledge base — use `axon query` + `axon retrieve` + `axon ask` via MCP or CLI",
                    fontSize = 10.sp,
                    color = Color(0xFFA7BCC9),
                    lineHeight = 15.sp,
                )
            }
        }
    }
}
