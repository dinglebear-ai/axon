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
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.automirrored.rounded.Send
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf

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
    val colors = AxonTheme.colors
    val tone = colors.toneOf(if (op.isAsync) AxonTone.Orange else AxonTone.Cyan)

    LaunchedEffect(op) {
        delay(80)
        focusRequester.requestFocus()
    }

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
                .padding(horizontal = 14.dp)
                .background(colors.panelStrong, RoundedCornerShape(20.dp))
                .border(1.dp, colors.tint(colors.accentPrimary, 35, colors.panelStrong), RoundedCornerShape(20.dp))
                .padding(16.dp)
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = {}),
            verticalArrangement = Arrangement.spacedBy(13.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(11.dp)) {
                Box(
                    modifier = Modifier
                        .size(38.dp)
                        .background(
                            colors.tint(tone.base, 12, colors.panelStrong),
                            RoundedCornerShape(12.dp),
                        )
                        .border(1.dp, colors.tint(tone.base, 28, colors.panelStrong), RoundedCornerShape(12.dp)),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(op.icon, contentDescription = null, tint = tone.fg, modifier = Modifier.size(19.dp))
                }
                Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                    Text(op.label, fontSize = 16.sp, fontWeight = FontWeight.ExtraBold, color = colors.textPrimary, fontFamily = AxonTheme.fonts.display)
                    Text(op.shortDescription(), fontSize = 11.5.sp, color = colors.textMuted, fontFamily = AxonTheme.fonts.body)
                }
                if (op.isAsync) {
                    Text(
                        "ASYNC",
                        fontSize = 9.sp,
                        fontWeight = FontWeight.Bold,
                        color = colors.orangeStrong,
                        fontFamily = AxonTheme.fonts.mono,
                        modifier = Modifier
                            .border(1.dp, colors.tint(colors.orange, 34, colors.panelStrong), RoundedCornerShape(5.dp))
                            .padding(horizontal = 6.dp, vertical = 2.dp),
                    )
                }
            }

            Row(
                modifier = Modifier
                    .height(46.dp)
                    .background(colors.control, RoundedCornerShape(13.dp))
                    .border(1.dp, colors.tint(tone.base, 34, colors.control), RoundedCornerShape(13.dp))
                    .border(3.dp, colors.tint(colors.accentPrimary, 16, colors.control), RoundedCornerShape(14.dp))
                    .padding(start = 12.dp, end = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                BasicTextField(
                    value = input,
                    onValueChange = { input = it },
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(focusRequester),
                    singleLine = true,
                    textStyle = TextStyle(
                        color = colors.textPrimary,
                        fontSize = 14.sp,
                        fontFamily = if (op == FabOp.Query || op == FabOp.Research || op == FabOp.Search) AxonTheme.fonts.body else AxonTheme.fonts.mono,
                    ),
                    cursorBrush = SolidColor(tone.base),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                    keyboardActions = KeyboardActions(onSend = {
                        if (input.isNotBlank()) onSubmit(input.trim())
                    }),
                    decorationBox = { inner ->
                        if (input.isBlank()) Text(op.placeholder, fontSize = 14.sp, color = colors.textMuted, fontFamily = AxonTheme.fonts.body)
                        inner()
                    },
                )

                IconButton(
                    onClick = {
                        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                        val text = cm.primaryClip?.getItemAt(0)?.text?.toString() ?: return@IconButton
                        input = text
                    },
                    modifier = Modifier.size(36.dp)
                        .background(Color.Transparent, RoundedCornerShape(8.dp)),
                ) {
                    Icon(Icons.Rounded.ContentCopy, contentDescription = "Paste", tint = colors.textMuted, modifier = Modifier.size(15.dp))
                }

                IconButton(
                    onClick = { if (input.isNotBlank()) onSubmit(input.trim()) },
                    modifier = Modifier.size(34.dp).background(tone.base, RoundedCornerShape(10.dp)),
                ) {
                    Icon(Icons.AutoMirrored.Rounded.Send, contentDescription = "Send", tint = Color(0xFF051520), modifier = Modifier.size(15.dp))
                }
            }

            Row(verticalAlignment = Alignment.CenterVertically) {
                Row(
                    modifier = Modifier.clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = null, tint = colors.textMuted, modifier = Modifier.size(13.dp))
                    Text("operations", fontSize = 11.5.sp, color = colors.textMuted, fontFamily = AxonTheme.fonts.body)
                }
                Spacer(Modifier.weight(1f))
                Text("enter to send · tap outside to cancel", fontSize = 10.sp, color = colors.textMuted.copy(alpha = 0.7f), fontFamily = AxonTheme.fonts.body)
            }
        }
    }
}

private fun FabOp.shortDescription(): String = when (this) {
    FabOp.Scrape -> "Fetch one page → markdown"
    FabOp.Research -> "Search + synthesize"
    FabOp.Extract -> "Structured extraction"
    FabOp.Query -> "Semantic vector search"
    FabOp.Search -> "Web search + index"
    FabOp.Map -> "Discover site URLs"
    FabOp.Retrieve -> "Fetch indexed chunks"
    FabOp.Summarize -> "Summarize a document"
    FabOp.Crawl -> "Async multi-page crawl"
    FabOp.Ingest -> "Import repo, reddit, or media"
}
