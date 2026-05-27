package com.axon.app.ui.tools

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import tv.tootie.aurora.components.AuroraButton
import tv.tootie.aurora.components.AuroraTextField

/**
 * URL field + submit button shared by the Scrape and Map tabs. The trimmed URL
 * is passed to [onSubmit] when the button is pressed.
 */
@Composable
fun ToolUrlForm(
    buttonLabel: String,
    submitEnabled: Boolean,
    onSubmit: (String) -> Unit,
    placeholder: String = "https://example.com",
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

    AuroraButton(
        onClick = { onSubmit(urlInput.trim()) },
        enabled = submitEnabled,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Text(buttonLabel)
    }
}
