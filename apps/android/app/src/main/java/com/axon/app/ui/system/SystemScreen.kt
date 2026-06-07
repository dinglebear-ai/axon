package com.axon.app.ui.system

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.HumanJsonRow
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.common.humanRows
import com.axon.app.ui.theme.AxonTheme
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant

@Composable
fun SystemScreen(vm: SystemViewModel = viewModel()) {
    val state by vm.doctor.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier.fillMaxSize().padding(horizontal = 14.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("System", color = AxonTheme.colors.textPrimary, fontSize = 18.sp, fontWeight = FontWeight.Bold, fontFamily = AxonTheme.fonts.display)
                Text("Doctor health check", color = AxonTheme.colors.textMuted, fontSize = 11.5.sp, fontFamily = AxonTheme.fonts.mono)
            }
            AuroraButton(onClick = { vm.refresh() }, variant = AuroraButtonVariant.Outlined) {
                Icon(Icons.Rounded.Refresh, contentDescription = null)
                Text("Refresh")
            }
        }

        when (val s = state) {
            Resource.Idle, Resource.Loading -> LoadingContent(
                label = "Running doctor…",
                modifier = Modifier.fillMaxWidth(),
            )
            is Resource.Error -> ErrorContent(message = s.message, onRetry = { vm.refresh() })
            is Resource.Ready -> {
                val rows = remember(s.value) { s.value.humanRows() }
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    items(rows, key = { "${it.depth}-${it.label}-${it.value}" }) { row ->
                        DoctorRow(row)
                    }
                }
            }
        }
    }
}

@Composable
private fun DoctorRow(row: HumanJsonRow) {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(11.dp))
            .background(colors.control.copy(alpha = 0.6f))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.7f), RoundedCornerShape(11.dp))
            .padding(start = (12 + row.depth.coerceAtMost(3) * 14).dp, top = 10.dp, end = 12.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            row.label,
            color = colors.textPrimary,
            fontSize = 12.sp,
            fontFamily = AxonTheme.fonts.body,
            modifier = Modifier.weight(1f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            row.value,
            color = colors.textMuted,
            fontSize = 11.sp,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
