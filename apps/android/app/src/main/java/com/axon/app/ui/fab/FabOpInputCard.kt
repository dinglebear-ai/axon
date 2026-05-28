package com.axon.app.ui.fab

import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private val BorderStrong  = Color(0xFF24536C)
private val AccentPrimary = Color(0xFF29B6F6)
private val AccentButton  = Color(0xFF1DA8E6)
private val AccentFg      = Color(0xFF051520)
private val PanelStrong   = Color(0xFF13293A)
private val CtrlSurface   = Color(0xFF0C1A24)
private val TextPrimary   = Color(0xFFE6F4FB)
private val TextMuted     = Color(0xFFA7BCC9)

@Composable
fun FabOpInputCard(
    op: FabOp,
    onSubmit: (input: String) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var input by remember { mutableStateOf("") }
    val focusRequester = remember { FocusRequester() }
    val context = LocalContext.current

    LaunchedEffect(op) { focusRequester.requestFocus() }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(Color(0x99040A0E))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .background(PanelStrong, RoundedCornerShape(20.dp))
                .border(1.dp, AccentPrimary.copy(alpha = 0.35f), RoundedCornerShape(20.dp))
                .padding(14.dp)
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = {}),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(
                    modifier = Modifier
                        .background(
                            if (op.isAsync) asyncOpTint.copy(0.12f) else AccentPrimary.copy(0.12f),
                            RoundedCornerShape(999.dp),
                        )
                        .border(1.dp, if (op.isAsync) asyncOpTint.copy(.25f) else AccentPrimary.copy(.25f), RoundedCornerShape(999.dp))
                        .padding(horizontal = 10.dp, vertical = 4.dp),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                        Icon(
                            imageVector = op.icon,
                            contentDescription = null,
                            tint = if (op.isAsync) asyncOpTint else AccentPrimary,
                            modifier = Modifier.size(14.dp),
                        )
                        Text(
                            op.label,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.SemiBold,
                            color = if (op.isAsync) asyncOpTint else AccentPrimary,
                        )
                    }
                }
            }

            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                OutlinedTextField(
                    value = input,
                    onValueChange = { input = it },
                    placeholder = { Text(op.placeholder, fontSize = 11.sp, color = TextMuted) },
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(focusRequester),
                    singleLine = true,
                    textStyle = LocalTextStyle.current.copy(fontSize = 12.sp, color = TextPrimary),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                    keyboardActions = KeyboardActions(onSend = {
                        if (input.isNotBlank()) onSubmit(input.trim())
                    }),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = AccentPrimary,
                        unfocusedBorderColor = BorderStrong,
                        focusedContainerColor = CtrlSurface,
                        unfocusedContainerColor = CtrlSurface,
                        cursorColor = AccentPrimary,
                    ),
                    shape = RoundedCornerShape(10.dp),
                )

                IconButton(
                    onClick = {
                        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                        val text = cm.primaryClip?.getItemAt(0)?.text?.toString() ?: return@IconButton
                        input = text
                    },
                    modifier = Modifier.size(36.dp)
                        .background(CtrlSurface, RoundedCornerShape(10.dp))
                        .border(1.dp, BorderStrong, RoundedCornerShape(10.dp)),
                ) {
                    Icon(Icons.Rounded.ContentPaste, contentDescription = "Paste", tint = TextMuted, modifier = Modifier.size(16.dp))
                }

                IconButton(
                    onClick = { if (input.isNotBlank()) onSubmit(input.trim()) },
                    modifier = Modifier.size(36.dp).background(AccentButton, RoundedCornerShape(10.dp)),
                ) {
                    Icon(Icons.Rounded.ArrowUpward, contentDescription = "Send", tint = AccentFg, modifier = Modifier.size(16.dp))
                }
            }

            Text(
                "enter to send · tap outside to cancel",
                fontSize = 9.sp,
                color = TextMuted.copy(alpha = 0.55f),
            )
        }
    }
}
