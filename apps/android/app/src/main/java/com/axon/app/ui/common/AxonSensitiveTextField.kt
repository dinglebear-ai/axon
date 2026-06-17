package com.axon.app.ui.common

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import tv.tootie.aurora.components.AuroraIconButton
import tv.tootie.aurora.components.AuroraIconButtonSize
import tv.tootie.aurora.components.AuroraTextField

private const val REDACTED_VALUE = "••••••••"

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
    var revealed by remember { mutableStateOf(false) }
    val hidden = !revealed && value.isNotEmpty()

    AuroraTextField(
        value = if (hidden) REDACTED_VALUE else value,
        onValueChange = { if (!hidden) onValueChange(it) },
        modifier = modifier,
        label = label,
        placeholder = placeholder,
        enabled = true,
        readOnly = hidden,
        singleLine = true,
        compact = compact,
        contentDescription = contentDescription,
        leadingIcon = leadingIcon,
        trailingIcon = {
            AuroraIconButton(
                onClick = { revealed = !revealed },
                imageVector = if (revealed) Icons.Filled.VisibilityOff else Icons.Filled.Visibility,
                contentDescription = if (revealed) hideContentDescription else revealContentDescription,
                size = AuroraIconButtonSize.Compact,
            )
        },
    )
}
