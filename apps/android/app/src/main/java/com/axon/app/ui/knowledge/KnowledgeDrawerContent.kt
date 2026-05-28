package com.axon.app.ui.knowledge

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Hub
import androidx.compose.material.icons.rounded.Lightbulb
import androidx.compose.material.icons.rounded.OpenInFull
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.Resource
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant

@Composable
fun KnowledgeDrawerContent(
    onOpenSuggest: () -> Unit,
    vm: KnowledgeViewModel = viewModel(),
) {
    val domains by vm.domains.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { vm.loadDomains() }

    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // Suggest shortcut card
        AuroraCard(
            onClick = onOpenSuggest,
            modifier = Modifier.fillMaxWidth(),
            variant = AuroraCardVariant.Outlined,
        ) {
            Row(
                modifier = Modifier.padding(12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Icon(Icons.Rounded.Lightbulb, contentDescription = null, tint = Color(0xFF29B6F6), modifier = Modifier.size(16.dp))
                Text("Suggest URLs", fontSize = 13.sp, color = Color(0xFFE6F4FB), modifier = Modifier.weight(1f))
                Icon(Icons.Rounded.OpenInFull, contentDescription = null, tint = Color(0xFF4A6374), modifier = Modifier.size(12.dp))
            }
        }

        // Domain count summary
        when (val d = domains) {
            is Resource.Ready -> {
                val count = d.value.size
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Icon(Icons.Rounded.Hub, contentDescription = null, tint = Color(0xFF4A6374), modifier = Modifier.size(14.dp))
                    Text(
                        "$count indexed domain${if (count == 1) "" else "s"}",
                        fontSize = 11.sp,
                        color = Color(0xFF4A6374),
                    )
                }
            }
            else -> { /* loading / error — no summary shown */ }
        }

        // Vector store shortcut row
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(Icons.Rounded.Storage, contentDescription = null, tint = Color(0xFF4A6374), modifier = Modifier.size(14.dp))
            Text(
                "Sources · Domains · Stats",
                fontSize = 11.sp,
                color = Color(0xFF4A6374),
            )
        }
    }
}
