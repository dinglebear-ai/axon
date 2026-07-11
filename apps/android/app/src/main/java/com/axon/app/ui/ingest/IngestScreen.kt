package com.axon.app.ui.ingest

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CloudUpload
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.JobIdChip
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.RecoveryActionCard
import com.axon.app.feature.jobs.JobRow
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraButtonVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraSelect
import tv.tootie.aurora.components.AuroraSeparator
import tv.tootie.aurora.components.AuroraTextField

/** Statuses where the server still allows a cancel request. Mirrors [com.axon.app.feature.jobs.CANCELABLE_STATUSES]. */
private val CANCELABLE_STATUSES = setOf("pending", "queued", "running", "in_progress")

/**
 * Ingest mode: submit an async ingest job (Github/Gitlab/Gitea/Git/Reddit/Youtube),
 * persist the returned jobId for the Jobs page, and offer one-shot status + cancel.
 *
 * UI is Aurora-only — [AuroraSelect] for the source picker, [AuroraTextField] for the
 * target, [AuroraButton] for actions. State machine routed through [IngestUi].
 */
@Composable
fun IngestScreen(vm: IngestViewModel = viewModel()) {
    val state by vm.uiState.collectAsStateWithLifecycle()
    var selectedSource by remember { mutableStateOf(IngestSource.Github) }
    var target by remember { mutableStateOf("") }

    val sourceOptions = remember { IngestSource.entries.map { it.name } }

    Column(
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Ingest", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()

        when (val s = state) {
            IngestUi.Idle -> IngestForm(
                selectedSource = selectedSource,
                onSourceChange = { selectedSource = it },
                target = target,
                onTargetChange = { target = it },
                sourceOptions = sourceOptions,
                onSubmit = { vm.submit(selectedSource, target.trim()) },
            )
            IngestUi.Submitting -> LoadingContent(
                label = "Submitting ingest job…",
                modifier = Modifier.fillMaxWidth(),
            )
            is IngestUi.Submitted -> SubmittedCard(
                jobId = s.jobId,
                target = s.target,
                onCheckStatus = { vm.checkStatus(s.jobId) },
                onCancel = { vm.cancel(s.jobId) },
                onReset = {
                    target = ""
                    vm.reset()
                },
            )
            is IngestUi.Status -> Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                JobRow(
                    job = s.job,
                    onCancel = if (s.job.status.lowercase() in CANCELABLE_STATUSES) {
                        { vm.cancel(s.job.id) }
                    } else null,
                )
                AuroraButton(
                    onClick = {
                        target = ""
                        vm.reset()
                    },
                    variant = AuroraButtonVariant.Outlined,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Submit another") }
            }
            is IngestUi.Error -> Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                RecoveryActionCard(
                    title = "Ingest could not start",
                    message = s.message,
                    primaryLabel = "Edit request",
                    onPrimary = vm::reset,
                    icon = Icons.Outlined.CloudUpload,
                )
                IngestForm(
                    selectedSource = selectedSource,
                    onSourceChange = { selectedSource = it },
                    target = target,
                    onTargetChange = { target = it },
                    sourceOptions = sourceOptions,
                    onSubmit = { vm.submit(selectedSource, target.trim()) },
                )
            }
        }
    }
}

@Composable
private fun IngestForm(
    selectedSource: IngestSource,
    onSourceChange: (IngestSource) -> Unit,
    target: String,
    onTargetChange: (String) -> Unit,
    sourceOptions: List<String>,
    onSubmit: () -> Unit,
) {
    val submitEnabled = target.isNotBlank()

    Column(verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxWidth()) {
        EmptyContent(
            title = "Index a new source",
            description = "Pick a source type and paste a target — repo URL, subreddit, channel, etc.",
            icon = Icons.Outlined.CloudUpload,
            modifier = Modifier.fillMaxWidth(),
        )
        AuroraSelect(
            selectedOption = selectedSource.name,
            onOptionSelected = { picked ->
                runCatching { IngestSource.valueOf(picked) }.getOrNull()?.let(onSourceChange)
            },
            options = sourceOptions,
            label = "Source",
            modifier = Modifier.fillMaxWidth(),
        )
        AuroraTextField(
            value = target,
            onValueChange = onTargetChange,
            label = "Target",
            placeholder = selectedSource.targetHostHint?.let { "e.g. https://$it/owner/repo" }
                ?: "Target URL or identifier",
            modifier = Modifier.fillMaxWidth(),
        )
        AuroraButton(
            onClick = onSubmit,
            enabled = submitEnabled,
            modifier = Modifier.fillMaxWidth(),
        ) { Text("Submit") }
    }
}

@Composable
private fun SubmittedCard(
    jobId: String,
    target: String,
    onCheckStatus: () -> Unit,
    onCancel: () -> Unit,
    onReset: () -> Unit,
) {
    AuroraCard(modifier = Modifier.fillMaxWidth(), variant = AuroraCardVariant.Outlined) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("Job submitted", style = MaterialTheme.typography.titleSmall)
            Text(target, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("ID:", style = MaterialTheme.typography.labelSmall)
                JobIdChip(jobId)
            }
            AuroraButton(
                onClick = onCheckStatus,
                variant = AuroraButtonVariant.Outlined,
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Check status") }
            AuroraButton(
                onClick = onCancel,
                variant = AuroraButtonVariant.Destructive,
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Cancel") }
            AuroraButton(
                onClick = onReset,
                variant = AuroraButtonVariant.Ghost,
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Submit another") }
        }
    }
}
