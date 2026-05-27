package com.axon.app.ui.summarize

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Notes
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
import com.axon.app.data.repository.SummarizeResultUi
import com.axon.app.ui.common.EmptyContent
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.operations.modeOptionsCog
import tv.tootie.aurora.components.AuroraCallout
import tv.tootie.aurora.components.AuroraCalloutVariant
import tv.tootie.aurora.components.AuroraCard
import tv.tootie.aurora.components.AuroraCardVariant
import tv.tootie.aurora.components.AuroraPromptInput
import tv.tootie.aurora.components.AuroraSeparator

@Composable
fun SummarizeScreen(vm: SummarizeViewModel = viewModel()) {
    val state by vm.uiState.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }

    Column(
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Summarize", style = MaterialTheme.typography.headlineMedium)
        AuroraSeparator()

        when (val s = state) {
            Resource.Idle -> EmptyContent(
                title = "Summarize a URL",
                description = "Paste a URL and I'll synthesise a summary via the configured LLM.",
                icon = Icons.Outlined.Notes,
                modifier = Modifier.fillMaxWidth(),
            )
            Resource.Loading -> LoadingContent(
                label = "Synthesising — may take a minute…",
                modifier = Modifier.fillMaxWidth(),
            )
            is Resource.Error -> ErrorContent(message = s.message)
            is Resource.Ready<*> -> {
                @Suppress("UNCHECKED_CAST")
                val ready = s as Resource.Ready<SummarizeResultUi>
                if (ready.value.contextTruncated) {
                    AuroraCallout(
                        title = "Context truncated",
                        message = "The source content was larger than the synthesis budget.",
                        variant = AuroraCalloutVariant.Warn,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                AuroraCard(
                    modifier = Modifier.fillMaxWidth(),
                    variant = AuroraCardVariant.Outlined,
                ) {
                    Column(modifier = Modifier.padding(12.dp).verticalScroll(rememberScrollState())) {
                        SelectionContainer {
                            Text(ready.value.summary, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
        }

        Spacer(Modifier.weight(1f, fill = false))
        AuroraSeparator()
        AuroraPromptInput(
            value = input,
            onValueChange = { input = it },
            onSend = {
                vm.submit(input.trim())
                input = ""
            },
            placeholder = "https://…",
            loading = state is Resource.Loading,
            actionLeft = modeOptionsCog(),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
