package com.axon.app.ui.fab

import android.content.ClipboardManager
import android.content.Context
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import com.axon.app.ui.common.pressScale
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
    var broadActionConfirmed by remember(op) { mutableStateOf(false) }
    val focusRequester = remember { FocusRequester() }
    val context = LocalContext.current
    val keyboardController = LocalSoftwareKeyboardController.current
    val colors = AxonTheme.colors
    val tone = colors.toneOf(if (op.isAsync) AxonTone.Orange else AxonTone.Cyan)
    val canSend = fabInputCanSubmit(op, input, broadActionConfirmed)

    LaunchedEffect(op) {
        input = ""
        // Drop straight into typing once the op is chosen — no extra tap to focus.
        delay(80)
        focusRequester.requestFocus()
        keyboardController?.show()
    }

    // The card blooms in after the op is picked from the ring — scrim fades up
    // while the panel springs from slightly small and low.
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }
    val enter by animateFloatAsState(
        targetValue = if (shown) 1f else 0f,
        animationSpec = spring(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessMedium),
        label = "fab-card-enter",
    )
    val cardSlidePx = with(LocalDensity.current) { 18.dp.toPx() }

    fun submitIfReady() {
        val normalized = normalizeFabInput(op, input)
        if (canSend) onSubmit(normalized)
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.scrim.copy(alpha = 0.90f * enter.coerceIn(0f, 1f)))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .imePadding()
                .navigationBarsPadding()
                .padding(start = 14.dp, end = 14.dp, bottom = 22.dp),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth(0.70f)
                    .widthIn(max = 318.dp)
                    .graphicsLayer {
                        val e = enter.coerceIn(0f, 1f)
                        alpha = e
                        val s = 0.92f + 0.08f * e
                        scaleX = s
                        scaleY = s
                        translationY = (1f - e) * cardSlidePx
                    }
                    .background(colors.panelStrong.copy(alpha = 0.46f), RoundedCornerShape(13.dp))
                    .border(1.dp, colors.tint(tone.base, 12, colors.panelStrong).copy(alpha = 0.52f), RoundedCornerShape(13.dp))
                    .padding(horizontal = 11.dp, vertical = 11.dp)
                    .clickable(remember { MutableInteractionSource() }, indication = null, onClick = {}),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Box(
                        modifier = Modifier
                            .size(28.dp)
                            .background(
                                colors.tint(tone.base, 7, colors.panelStrong),
                                RoundedCornerShape(7.dp),
                            )
                            .border(1.dp, colors.tint(tone.base, 14, colors.panelStrong), RoundedCornerShape(7.dp)),
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(op.icon, contentDescription = null, tint = tone.fg.copy(alpha = 0.84f), modifier = Modifier.size(14.dp))
                    }
                    Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(0.dp)) {
                        Text(op.label, fontSize = 13.8.sp, fontWeight = FontWeight.ExtraBold, color = colors.textPrimary.copy(alpha = 0.90f), fontFamily = AxonTheme.fonts.display)
                        Text(op.shortDescription(), fontSize = 10.sp, color = colors.textMuted.copy(alpha = 0.58f), fontFamily = AxonTheme.fonts.body)
                    }
                    if (op.isAsync) {
                        Text(
                            "ASYNC",
                            fontSize = 7.4.sp,
                            fontWeight = FontWeight.Bold,
                            color = colors.orangeStrong,
                            fontFamily = AxonTheme.fonts.mono,
                            modifier = Modifier
                                .border(1.dp, colors.tint(colors.orange, 24, colors.panelStrong), RoundedCornerShape(5.dp))
                                .padding(horizontal = 5.dp, vertical = 1.5.dp),
                        )
                    }
                }

                Row(
                    modifier = Modifier
                        .height(42.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .background(colors.control.copy(alpha = 0.80f), RoundedCornerShape(10.dp))
                        .border(1.dp, colors.tint(tone.base, 22, colors.control), RoundedCornerShape(10.dp))
                        .clickable(remember { MutableInteractionSource() }, indication = null) {
                            focusRequester.requestFocus()
                            keyboardController?.show()
                        }
                        .padding(start = 10.dp, end = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    BasicTextField(
                        value = input,
                        onValueChange = {
                            input = it
                            broadActionConfirmed = false
                        },
                        modifier = Modifier
                            .weight(1f)
                            .focusRequester(focusRequester),
                        singleLine = true,
                        textStyle = TextStyle(
                            color = colors.textPrimary,
                            fontSize = 12.6.sp,
                            fontFamily = if (op == FabOp.Query || op == FabOp.Research || op == FabOp.Search) AxonTheme.fonts.body else AxonTheme.fonts.mono,
                        ),
                        cursorBrush = SolidColor(tone.base),
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                        keyboardActions = KeyboardActions(onSend = {
                            submitIfReady()
                        }),
                        decorationBox = { inner ->
                            if (input.isBlank()) Text(op.placeholder, fontSize = 12.6.sp, color = colors.textMuted.copy(alpha = 0.56f), fontFamily = AxonTheme.fonts.body)
                            inner()
                        },
                    )

                    Box(
                        modifier = Modifier
                            .size(28.dp)
                            .pressScale {
                                val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                val text = cm.primaryClip?.getItemAt(0)?.text?.toString()
                                if (text != null) {
                                    input = text
                                    broadActionConfirmed = false
                                }
                            }
                            .background(Color.Transparent, RoundedCornerShape(8.dp)),
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(Icons.Rounded.ContentCopy, contentDescription = "Paste", tint = colors.textMuted.copy(alpha = 0.54f), modifier = Modifier.size(13.dp))
                    }

                    val sendBgAlpha by animateFloatAsState(
                        targetValue = if (canSend) 0.90f else 0.34f,
                        animationSpec = tween(durationMillis = 160),
                        label = "fab-send-alpha",
                    )
                    Box(
                        modifier = Modifier
                            .size(32.dp)
                            .pressScale(enabled = canSend) {
                                submitIfReady()
                            }
                            .background(tone.base.copy(alpha = sendBgAlpha), RoundedCornerShape(9.dp)),
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(
                            Icons.AutoMirrored.Rounded.Send,
                            contentDescription = "Send",
                            tint = colors.onAccentFg.copy(alpha = if (canSend) 1f else 0.55f),
                            modifier = Modifier.size(15.dp),
                        )
                    }
                }

                op.broadActionConfirmationLabel()?.let { label ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .pressScale {
                                broadActionConfirmed = !broadActionConfirmed
                            },
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(7.dp),
                    ) {
                        Box(
                            modifier = Modifier
                                .size(15.dp)
                                .background(
                                    if (broadActionConfirmed) tone.base.copy(alpha = 0.92f) else Color.Transparent,
                                    RoundedCornerShape(4.dp),
                                )
                                .border(
                                    1.dp,
                                    colors.tint(tone.base, 28, colors.panelStrong).copy(alpha = 0.82f),
                                    RoundedCornerShape(4.dp),
                                ),
                            contentAlignment = Alignment.Center,
                        ) {
                            if (broadActionConfirmed) {
                                Icon(
                                    Icons.Rounded.Check,
                                    contentDescription = null,
                                    tint = colors.onAccentFg,
                                    modifier = Modifier.size(11.dp),
                                )
                            }
                        }
                        Text(
                            label,
                            fontSize = 9.8.sp,
                            color = colors.textMuted.copy(alpha = 0.82f),
                            fontFamily = AxonTheme.fonts.mono,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )
                    }
                }

                Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Row(
                        modifier = Modifier.pressScale(onClick = onDismiss),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = null, tint = colors.textMuted.copy(alpha = 0.62f), modifier = Modifier.size(12.dp))
                        Text("operations", fontSize = 10.4.sp, color = colors.textMuted.copy(alpha = 0.74f), fontFamily = AxonTheme.fonts.body)
                    }
                    Text(
                        if (op.broadActionConfirmationLabel() != null && !broadActionConfirmed) {
                            "confirm options to send · tap outside to cancel"
                        } else {
                            "enter to send · tap outside to cancel"
                        },
                        fontSize = 9.4.sp,
                        color = colors.textMuted.copy(alpha = 0.64f),
                        fontFamily = AxonTheme.fonts.mono,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
    }
}

internal fun normalizeFabInput(op: FabOp, input: String): String {
    val trimmed = input.trim()
    if (trimmed.isBlank()) return ""
    return if (op.expectsUrl() && !trimmed.contains("://")) "https://$trimmed" else trimmed
}

internal fun fabInputCanSubmit(
    op: FabOp,
    input: String,
    broadActionConfirmed: Boolean,
): Boolean {
    val normalized = normalizeFabInput(op, input)
    return normalized.isNotBlank() && (op.broadActionConfirmationLabel() == null || broadActionConfirmed)
}

private fun FabOp.expectsUrl(): Boolean = when (this) {
    FabOp.Scrape,
    FabOp.Extract,
    FabOp.Map,
    FabOp.Retrieve,
    FabOp.Summarize,
    FabOp.Crawl -> true
    FabOp.Research,
    FabOp.Embed,
    FabOp.Query,
    FabOp.Search,
    FabOp.Ingest -> false
}

private fun FabOp.shortDescription(): String = when (this) {
    FabOp.Scrape -> "Fetch one page → markdown"
    FabOp.Research -> "Search + synthesize"
    FabOp.Extract -> "Structured extraction"
    FabOp.Embed -> "Index content"
    FabOp.Query -> "Semantic vector search"
    FabOp.Search -> "Web search + index"
    FabOp.Map -> "Discover site URLs"
    FabOp.Retrieve -> "Fetch indexed chunks"
    FabOp.Summarize -> "Summarize a document"
    FabOp.Crawl -> "Async multi-page crawl"
    FabOp.Ingest -> "Import repo, reddit, or media"
}

internal fun FabOp.broadActionConfirmationLabel(): String? = when (this) {
    FabOp.Crawl -> "Run with current crawl defaults/options"
    FabOp.Ingest -> "Run with current ingest defaults/options"
    else -> null
}
