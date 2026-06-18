package com.axon.app.ui.common

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import tv.tootie.aurora.components.AuroraTextField

@Composable
fun AxonSensitiveTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    label: String? = null,
    placeholder: String? = null,
    compact: Boolean = false,
    contentDescription: String? = label,
    revealContentDescription: String = "Show value",
    hideContentDescription: String = "Hide value",
    leadingIcon: (@Composable () -> Unit)? = null,
) {
    AuroraTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier,
        label = label,
        placeholder = placeholder,
        enabled = true,
        singleLine = true,
        compact = compact,
        sensitive = true,
        initiallyRevealed = false,
        revealContentDescription = revealContentDescription,
        hideContentDescription = hideContentDescription,
        contentDescription = contentDescription,
        leadingIcon = leadingIcon,
    )
}
