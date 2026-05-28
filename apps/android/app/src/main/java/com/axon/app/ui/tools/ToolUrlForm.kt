package com.axon.app.ui.tools

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraTextField

/**
 * URL field + submit button shared by the Scrape, Map, Crawl, and Research tabs.
 *
 * The trimmed URL is passed to [onSubmit] when the button is pressed. When
 * [actionLeft] is provided, it renders to the left of the submit button so a
 * secondary trigger (e.g. the mode-options cog) sits inline next to Submit.
 */
@Composable
fun ToolUrlForm(
    buttonLabel: String,
    submitEnabled: Boolean,
    onSubmit: (String) -> Unit,
    placeholder: String = "https://example.com",
    actionLeft: (@Composable () -> Unit)? = null,
) {
    var urlInput by remember { mutableStateOf("") }

    AuroraTextField(
        value = urlInput,
        onValueChange = { urlInput = it },
        label = "URL",
        placeholder = placeholder,
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )

    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        actionLeft?.invoke()
        AuroraButton(
            onClick = { onSubmit(urlInput.trim()) },
            enabled = submitEnabled,
            modifier = Modifier.weight(1f),
        ) {
            Text(buttonLabel)
        }
    }
}
