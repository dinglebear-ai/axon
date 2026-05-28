# Android Phase 3 — Redesign Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two FAB ring bugs (Back key exits app; items clip off screen) and wire the MGMT and SETUP drawer stubs with their five spec-defined sub-items each.

**Architecture:** Four isolated changes — (1) `BackHandler` in `FabLauncher`, (2) ring-center repositioned to screen center in `FabLauncher`/`FabRing`, (3) `ManagementDrawerContent` wired with live ConnectionStatus + sub-items, (4) `SetupDrawerContent` wired with smoke/doctor/settings sub-items. New ViewModels (`ManagementViewModel`, `SetupViewModel`) own the async calls. `DrawerSectionContent` passes nav lambdas to both stubs.

**Tech Stack:** Kotlin 2.1, Jetpack Compose BOM 2026.04.01, `androidx.activity.compose.BackHandler`, `BoxWithConstraints` (Compose foundation), existing `AxonClient` API (`healthz`, `doctor`, `stats`), `ConnectionStatusViewModel` (already wired).

**Spec:** `docs/specs/android-redesign.md`

---

## File Map

**Modify:**
```
apps/android/app/src/main/java/com/axon/app/
  ui/fab/FabLauncher.kt                 — add BackHandler; pass screen-center to FabRing when ring open
  ui/fab/FabRing.kt                     — add dim backdrop when visible
  ui/nav/DrawerSectionContent.kt        — pass onOpenSettings lambda to Management + Setup cases
  ui/management/ManagementDrawerContent.kt — replace stub with live content
  ui/setup/SetupDrawerContent.kt           — replace stub with live content
```

**Create:**
```
apps/android/app/src/main/java/com/axon/app/
  ui/management/ManagementViewModel.kt  — doctor + stats async calls
  ui/setup/SetupViewModel.kt            — smoke + doctor async calls
```

---

## Task 1: FAB ring — Back key dismissal

**File:** `apps/android/app/src/main/java/com/axon/app/ui/fab/FabLauncher.kt`

Currently, pressing Back while the FAB ring or input card is open exits the app instead of closing the ring. `BackHandler` must intercept Back when state is `Ring` or `Input`.

- [ ] **Step 1: Add the BackHandler import**

At the top of `FabLauncher.kt`, add one import:
```kotlin
import androidx.activity.compose.BackHandler
```

- [ ] **Step 2: Insert BackHandler inside the Box, before FabRing**

Find the `Box(modifier = modifier.fillMaxSize())` block in `FabLauncher`. Add the `BackHandler` call immediately after the opening brace, before the `FabRing` call:

```kotlin
Box(modifier = modifier.fillMaxSize()) {
    BackHandler(enabled = state !is FabState.Idle) {
        state = FabState.Idle
    }

    FabRing(
        visible = state is FabState.Ring,
        // ... rest unchanged
    )
    // ... rest unchanged
}
```

- [ ] **Step 3: Verify build compiles**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: `BUILD SUCCESSFUL`

- [ ] **Step 4: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/fab/FabLauncher.kt
git commit -m "fix(android): BackHandler dismisses FAB ring on back press"
```

---

## Task 2: FAB ring — Screen-centered ring (fixes clipping)

**Files:**
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/fab/FabLauncher.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/fab/FabRing.kt`

The FAB sits at `BottomEnd`. At radius 96dp, items at the 2–6 o'clock positions clip off the right and bottom edges. The spec says "FAB transforms to center of a full 360° ring" — the ring should appear screen-centered when open so all 10 items are visible. The `×` dismiss button at the ring center marks where the FAB was visually.

### 2a: Screen-center the ring in FabLauncher

- [ ] **Step 1: Add BoxWithConstraints import and replace Box root**

Replace the imports section top (keep existing imports, add):
```kotlin
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.ui.platform.LocalDensity
import kotlin.math.roundToInt
```

Replace the outer `Box(modifier = modifier.fillMaxSize())` with `BoxWithConstraints`:

```kotlin
@Composable
fun FabLauncher(
    onOpSubmit: (FabOp, String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var state by remember { mutableStateOf<FabState>(FabState.Idle) }
    var fabCenter by remember { mutableStateOf(IntOffset.Zero) }

    BackHandler(enabled = state !is FabState.Idle) {
        state = FabState.Idle
    }

    BoxWithConstraints(modifier = modifier.fillMaxSize()) {
        val density = LocalDensity.current
        val screenCenter = remember(maxWidth, maxHeight) {
            IntOffset(
                x = with(density) { (maxWidth / 2).roundToPx() },
                y = with(density) { (maxHeight / 2).roundToPx() },
            )
        }

        FabRing(
            visible = state is FabState.Ring,
            fabCenterOffset = if (state is FabState.Ring) screenCenter else fabCenter,
            onOpSelected = { op -> state = FabState.Input(op) },
            onDismiss = { state = FabState.Idle },
        )

        if (state is FabState.Input) {
            val op = (state as FabState.Input).op
            FabOpInputCard(
                op = op,
                onSubmit = { input ->
                    state = FabState.Idle
                    onOpSubmit(op, input)
                },
                onDismiss = { state = FabState.Idle },
            )
        }

        if (state is FabState.Idle) {
            Box(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(bottom = 80.dp, end = 16.dp)
                    .size(42.dp)
                    .onGloballyPositioned { coords ->
                        val pos = coords.positionInWindow()
                        fabCenter = IntOffset(
                            x = (pos.x + coords.size.width / 2).roundToInt(),
                            y = (pos.y + coords.size.height / 2).roundToInt(),
                        )
                    }
                    .background(Color(0xFF13293A), RoundedCornerShape(13.dp))
                    .border(1.dp, Color(0xFF24536C), RoundedCornerShape(13.dp))
                    .clickable(remember { MutableInteractionSource() }, indication = null) {
                        state = FabState.Ring
                    },
                contentAlignment = Alignment.Center,
            ) {
                Icon(Icons.Rounded.Add, contentDescription = "Launch operation", tint = Color(0xFF29B6F6), modifier = Modifier.size(20.dp))
            }
        }
    }
}
```

### 2b: Add dim backdrop to FabRing

- [ ] **Step 2: Add backdrop Box at the start of FabRing's Box**

In `FabRing.kt`, after `Box(modifier = modifier.fillMaxSize()) {`, add a full-screen dim layer before the item loops:

```kotlin
Box(modifier = modifier.fillMaxSize()) {
    // Dim backdrop — only drawn when ring has opened far enough to be meaningful
    if (openProgress > 0f) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Color(0xD2040A0E).copy(alpha = openProgress * 0.82f))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
        )
    }

    FabOp.entries.forEachIndexed { i, op ->
        // ... rest unchanged
    }
    // ... rest unchanged
}
```

- [ ] **Step 3: Add MutableInteractionSource import to FabRing.kt**

Ensure `FabRing.kt` has:
```kotlin
import androidx.compose.foundation.interaction.MutableInteractionSource
```
(It is already present — verify with `grep MutableInteraction` on the file.)

- [ ] **Step 4: Verify build compiles**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: `BUILD SUCCESSFUL`

- [ ] **Step 5: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/fab/FabLauncher.kt \
        apps/android/app/src/main/java/com/axon/app/ui/fab/FabRing.kt
git commit -m "fix(android): center FAB ring on screen so all 10 ops are visible; add dim backdrop"
```

---

## Task 3: ManagementViewModel

**File:** `apps/android/app/src/main/java/com/axon/app/ui/management/ManagementViewModel.kt` (create)

The Management drawer needs async calls for the Stack (stats) and Doctor sub-items. A ViewModel keeps the results across recompositions and scopes the coroutines to the drawer's lifetime.

- [ ] **Step 1: Create ManagementViewModel.kt**

```kotlin
package com.axon.app.ui.management

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

sealed interface MgmtActionState {
    data object Idle : MgmtActionState
    data object Loading : MgmtActionState
    data class Done(val summary: String) : MgmtActionState
    data class Error(val message: String) : MgmtActionState
}

class ManagementViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _statsState = MutableStateFlow<MgmtActionState>(MgmtActionState.Idle)
    val statsState: StateFlow<MgmtActionState> = _statsState.asStateFlow()

    private val _doctorState = MutableStateFlow<MgmtActionState>(MgmtActionState.Idle)
    val doctorState: StateFlow<MgmtActionState> = _doctorState.asStateFlow()

    fun loadStats() {
        if (_statsState.value is MgmtActionState.Loading) return
        viewModelScope.launch {
            _statsState.value = MgmtActionState.Loading
            container.axonClient.stats().fold(
                onSuccess = { resp ->
                    // payload is opaque JsonObject — show top-level key count as a summary
                    val preview = resp.payload.entries
                        .take(4)
                        .joinToString(" · ") { (k, v) -> "$k: $v" }
                    _statsState.value = MgmtActionState.Done(preview.ifBlank { "ok" })
                },
                onFailure = { e ->
                    _statsState.value = MgmtActionState.Error(e.message ?: "Stats unavailable")
                },
            )
        }
    }

    fun runDoctor() {
        if (_doctorState.value is MgmtActionState.Loading) return
        viewModelScope.launch {
            _doctorState.value = MgmtActionState.Loading
            container.axonClient.doctor().fold(
                onSuccess = { resp ->
                    val preview = resp.payload.toString().take(200)
                    _doctorState.value = MgmtActionState.Done(preview)
                },
                onFailure = { e ->
                    _doctorState.value = MgmtActionState.Error(e.message ?: "Doctor unavailable")
                },
            )
        }
    }
}
```

- [ ] **Step 2: Verify build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/management/ManagementViewModel.kt
git commit -m "feat(android): ManagementViewModel — stats + doctor async calls"
```

---

## Task 4: ManagementDrawerContent — wire spec sub-items

**File:** `apps/android/app/src/main/java/com/axon/app/ui/management/ManagementDrawerContent.kt`

Spec sub-items: **Dedupe · Monitor · Sync · Stack · Config**

- Monitor: shows live connection status (via `ConnectionStatusViewModel`)
- Stack: shows stats payload (via `ManagementViewModel.loadStats()`)
- Config: navigates to Settings via `onOpenSettings`
- Dedupe: placeholder row (server endpoint not yet wired — shows "Coming soon" badge)
- Sync: placeholder row (same reasoning)

- [ ] **Step 1: Rewrite ManagementDrawerContent.kt**

```kotlin
package com.axon.app.ui.management

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.status.ConnectionState
import com.axon.app.ui.status.ConnectionStatusViewModel

private val AccentPrimary = Color(0xFF29B6F6)
private val TextMuted     = Color(0xFFA7BCC9)
private val WarnBase      = Color(0xFFC6A36B)
private val ErrorBase     = Color(0xFFEF5350)
private val SuccessBase   = Color(0xFF66BB6A)
private val TextLabel     = Color(0xFFE1EEF7)

@Composable
fun ManagementDrawerContent(
    onOpenSettings: () -> Unit,
    statusVm: ConnectionStatusViewModel = viewModel(),
    vm: ManagementViewModel = viewModel(),
) {
    val connState by statusVm.state.collectAsStateWithLifecycle()
    val statsState by vm.statsState.collectAsStateWithLifecycle()
    val doctorState by vm.doctorState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        // ── Connection status header ──────────────────────────────────────────
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text("Server", style = MaterialTheme.typography.labelSmall, color = TextMuted)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                val (dotColor, label) = when (connState) {
                    ConnectionState.Checking -> AccentPrimary to "Checking"
                    ConnectionState.Online   -> SuccessBase to "Online"
                    ConnectionState.Offline  -> ErrorBase to "Offline"
                }
                Box(
                    modifier = Modifier
                        .size(7.dp)
                        .let { if (connState != ConnectionState.Offline) it else it },
                ) {
                    androidx.compose.foundation.Canvas(modifier = Modifier.fillMaxSize()) {
                        drawCircle(color = dotColor)
                    }
                }
                Text(label, style = MaterialTheme.typography.labelSmall, color = dotColor)
                Text("·", style = MaterialTheme.typography.labelSmall, color = TextMuted)
                Text(
                    "Refresh",
                    style = MaterialTheme.typography.labelSmall,
                    color = AccentPrimary,
                    modifier = Modifier.clickable(remember { MutableInteractionSource() }, indication = null) {
                        statusVm.refresh()
                    },
                )
            }
        }

        // ── Monitor ───────────────────────────────────────────────────────────
        MgmtSubItem(
            icon = Icons.Rounded.MonitorHeart,
            label = "Monitor",
            detail = when (connState) {
                ConnectionState.Checking -> "Checking…"
                ConnectionState.Online   -> "Server reachable"
                ConnectionState.Offline  -> "Server unreachable"
            },
            detailColor = when (connState) {
                ConnectionState.Checking -> TextMuted
                ConnectionState.Online   -> SuccessBase
                ConnectionState.Offline  -> ErrorBase
            },
        )

        // ── Stack (stats) ─────────────────────────────────────────────────────
        MgmtSubItem(
            icon = Icons.Rounded.Storage,
            label = "Stack",
            detail = when (val s = statsState) {
                is MgmtActionState.Idle    -> "Tap to load"
                is MgmtActionState.Loading -> "Loading…"
                is MgmtActionState.Done    -> s.summary
                is MgmtActionState.Error   -> s.message
            },
            detailColor = when (statsState) {
                is MgmtActionState.Error -> ErrorBase
                else -> TextMuted
            },
            onClick = { vm.loadStats() },
        )

        // ── Dedupe ────────────────────────────────────────────────────────────
        MgmtSubItem(
            icon = Icons.Rounded.ContentCopy,
            label = "Dedupe",
            detail = "Coming soon",
            detailColor = TextMuted,
            badgeLabel = "soon",
            badgeColor = WarnBase,
        )

        // ── Sync ──────────────────────────────────────────────────────────────
        MgmtSubItem(
            icon = Icons.Rounded.Sync,
            label = "Sync",
            detail = "Coming soon",
            detailColor = TextMuted,
            badgeLabel = "soon",
            badgeColor = WarnBase,
        )

        // ── Config ────────────────────────────────────────────────────────────
        MgmtSubItem(
            icon = Icons.Rounded.Tune,
            label = "Config",
            detail = "Server URL, token, collection",
            detailColor = TextMuted,
            onClick = onOpenSettings,
        )
    }
}

@Composable
private fun MgmtSubItem(
    icon: ImageVector,
    label: String,
    detail: String,
    detailColor: Color = TextMuted,
    badgeLabel: String? = null,
    badgeColor: Color = WarnBase,
    onClick: (() -> Unit)? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .let { if (onClick != null) it.clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick) else it }
            .padding(vertical = 8.dp, horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(imageVector = icon, contentDescription = label, tint = if (onClick != null) AccentPrimary else TextMuted, modifier = Modifier.size(17.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.bodySmall, color = TextLabel)
            Text(detail, style = MaterialTheme.typography.labelSmall, color = detailColor)
        }
        if (badgeLabel != null) {
            Text(
                badgeLabel,
                style = MaterialTheme.typography.labelSmall,
                color = badgeColor,
            )
        } else if (onClick != null) {
            Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = TextMuted, modifier = Modifier.size(14.dp))
        }
    }
}
```

- [ ] **Step 2: Verify build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/management/ManagementDrawerContent.kt
git commit -m "feat(android): ManagementDrawerContent — Monitor/Stack/Dedupe/Sync/Config sub-items"
```

---

## Task 5: SetupViewModel

**File:** `apps/android/app/src/main/java/com/axon/app/ui/setup/SetupViewModel.kt` (create)

The Setup drawer needs smoke (healthz) and doctor calls.

- [ ] **Step 1: Create SetupViewModel.kt**

```kotlin
package com.axon.app.ui.setup

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

sealed interface SetupActionState {
    data object Idle : SetupActionState
    data object Running : SetupActionState
    data class Pass(val detail: String) : SetupActionState
    data class Fail(val message: String) : SetupActionState
}

class SetupViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _smokeState = MutableStateFlow<SetupActionState>(SetupActionState.Idle)
    val smokeState: StateFlow<SetupActionState> = _smokeState.asStateFlow()

    private val _doctorState = MutableStateFlow<SetupActionState>(SetupActionState.Idle)
    val doctorState: StateFlow<SetupActionState> = _doctorState.asStateFlow()

    fun runSmoke() {
        if (_smokeState.value is SetupActionState.Running) return
        viewModelScope.launch {
            _smokeState.value = SetupActionState.Running
            container.axonClient.healthz().fold(
                onSuccess = { _smokeState.value = SetupActionState.Pass("/healthz → 200 OK") },
                onFailure = { e -> _smokeState.value = SetupActionState.Fail(e.message ?: "Unreachable") },
            )
        }
    }

    fun runDoctor() {
        if (_doctorState.value is SetupActionState.Running) return
        viewModelScope.launch {
            _doctorState.value = SetupActionState.Running
            container.axonClient.doctor().fold(
                onSuccess = { resp ->
                    val preview = resp.payload.toString().take(300).trimEnd(',')
                    _doctorState.value = SetupActionState.Pass(preview)
                },
                onFailure = { e ->
                    _doctorState.value = SetupActionState.Fail(e.message ?: "Doctor unavailable")
                },
            )
        }
    }
}
```

- [ ] **Step 2: Verify build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/setup/SetupViewModel.kt
git commit -m "feat(android): SetupViewModel — smoke + doctor async calls"
```

---

## Task 6: SetupDrawerContent — wire spec sub-items

**File:** `apps/android/app/src/main/java/com/axon/app/ui/setup/SetupDrawerContent.kt`

Spec sub-items: **Preflight · Setup · Smoke · Doctor · Debug**

- Preflight: runs healthz + doctor together (run smoke + doctor simultaneously) — shows pass/fail
- Setup: navigates to SettingsRoute via `onOpenSettings`
- Smoke: runs `/healthz` and shows pass/fail inline
- Doctor: runs `/v1/doctor` and shows summary
- Debug: taps to show raw `/v1/status` (navigates to settings since status is informational)

- [ ] **Step 1: Rewrite SetupDrawerContent.kt**

```kotlin
package com.axon.app.ui.setup

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel

private val AccentPrimary = Color(0xFF29B6F6)
private val TextMuted     = Color(0xFFA7BCC9)
private val SuccessBase   = Color(0xFF66BB6A)
private val ErrorBase     = Color(0xFFEF5350)
private val TextLabel     = Color(0xFFE1EEF7)

@Composable
fun SetupDrawerContent(
    onOpenSettings: () -> Unit,
    vm: SetupViewModel = viewModel(),
) {
    val smokeState by vm.smokeState.collectAsStateWithLifecycle()
    val doctorState by vm.doctorState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        // ── Preflight (smoke + doctor) ────────────────────────────────────────
        SetupSubItem(
            icon = Icons.Rounded.FlightTakeoff,
            label = "Preflight",
            detail = when {
                smokeState is SetupActionState.Running || doctorState is SetupActionState.Running -> "Running checks…"
                smokeState is SetupActionState.Fail    -> (smokeState as SetupActionState.Fail).message
                doctorState is SetupActionState.Fail   -> (doctorState as SetupActionState.Fail).message
                smokeState is SetupActionState.Pass && doctorState is SetupActionState.Pass -> "All checks passed"
                else -> "Tap to run all checks"
            },
            detailColor = when {
                smokeState is SetupActionState.Fail || doctorState is SetupActionState.Fail -> ErrorBase
                smokeState is SetupActionState.Pass && doctorState is SetupActionState.Pass -> SuccessBase
                else -> TextMuted
            },
            onClick = {
                vm.runSmoke()
                vm.runDoctor()
            },
        )

        // ── Setup (→ Settings) ────────────────────────────────────────────────
        SetupSubItem(
            icon = Icons.Rounded.Construction,
            label = "Setup",
            detail = "Server URL · Token · Collection",
            detailColor = TextMuted,
            onClick = onOpenSettings,
        )

        // ── Smoke ─────────────────────────────────────────────────────────────
        SetupSubItem(
            icon = Icons.Rounded.Wifi,
            label = "Smoke",
            detail = when (val s = smokeState) {
                is SetupActionState.Idle    -> "Tap to run /healthz"
                is SetupActionState.Running -> "Testing connectivity…"
                is SetupActionState.Pass    -> s.detail
                is SetupActionState.Fail    -> s.message
            },
            detailColor = when (smokeState) {
                is SetupActionState.Pass -> SuccessBase
                is SetupActionState.Fail -> ErrorBase
                else -> TextMuted
            },
            onClick = { vm.runSmoke() },
        )

        // ── Doctor ────────────────────────────────────────────────────────────
        SetupSubItem(
            icon = Icons.Rounded.HealthAndSafety,
            label = "Doctor",
            detail = when (val s = doctorState) {
                is SetupActionState.Idle    -> "Tap to run /v1/doctor"
                is SetupActionState.Running -> "Running diagnostics…"
                is SetupActionState.Pass    -> s.detail
                is SetupActionState.Fail    -> s.message
            },
            detailColor = when (doctorState) {
                is SetupActionState.Pass -> SuccessBase
                is SetupActionState.Fail -> ErrorBase
                else -> TextMuted
            },
            onClick = { vm.runDoctor() },
        )

        // ── Debug ─────────────────────────────────────────────────────────────
        SetupSubItem(
            icon = Icons.Rounded.BugReport,
            label = "Debug",
            detail = "Server config · Advanced settings",
            detailColor = TextMuted,
            onClick = onOpenSettings,
        )
    }
}

@Composable
private fun SetupSubItem(
    icon: ImageVector,
    label: String,
    detail: String,
    detailColor: Color = TextMuted,
    onClick: (() -> Unit)? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .let { if (onClick != null) it.clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick) else it }
            .padding(vertical = 8.dp, horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(imageVector = icon, contentDescription = label, tint = if (onClick != null) AccentPrimary else TextMuted, modifier = Modifier.size(17.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.bodySmall, color = TextLabel)
            Text(detail, style = MaterialTheme.typography.labelSmall, color = detailColor, maxLines = 2)
        }
        if (onClick != null) {
            Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = TextMuted, modifier = Modifier.size(14.dp))
        }
    }
}
```

- [ ] **Step 2: Verify build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/setup/SetupDrawerContent.kt
git commit -m "feat(android): SetupDrawerContent — Preflight/Setup/Smoke/Doctor/Debug sub-items"
```

---

## Task 7: Wire DrawerSectionContent

**File:** `apps/android/app/src/main/java/com/axon/app/ui/nav/DrawerSectionContent.kt`

Update the `Management` and `Setup` cases to pass the `onOpenSettings` lambda. The existing `navController` in scope routes to `SettingsRoute`.

- [ ] **Step 1: Add SettingsRoute import**

`SettingsRoute` is defined in `AxonNavGraph.kt` (same package `com.axon.app.ui.nav`) — no import needed since they're in the same package.

- [ ] **Step 2: Update the Management and Setup cases**

Replace the two stub cases:

```kotlin
DrawerSection.Management -> ManagementDrawerContent()
DrawerSection.Setup      -> SetupDrawerContent()
```

with:

```kotlin
DrawerSection.Management -> ManagementDrawerContent(
    onOpenSettings = { navController.navigate(SettingsRoute) },
)
DrawerSection.Setup -> SetupDrawerContent(
    onOpenSettings = { navController.navigate(SettingsRoute) },
)
```

- [ ] **Step 3: Add imports if missing**

Ensure the import for `ManagementDrawerContent` and `SetupDrawerContent` are present:
```kotlin
import com.axon.app.ui.management.ManagementDrawerContent
import com.axon.app.ui.setup.SetupDrawerContent
```
(Both are likely already imported — verify with `grep -n import DrawerSectionContent.kt`.)

- [ ] **Step 4: Verify build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: `BUILD SUCCESSFUL`

- [ ] **Step 5: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/nav/DrawerSectionContent.kt
git commit -m "feat(android): wire Management and Setup drawers with onOpenSettings nav"
```

---

## Task 8: Full build, install, and ADB smoke test

- [ ] **Step 1: Full debug build**

```bash
cd apps/android && ./gradlew assembleDebug --rerun-tasks 2>&1 | tail -10
```
Expected: `BUILD SUCCESSFUL` — APK at `app/build/outputs/apk/debug/app-debug.apk`

- [ ] **Step 2: Install on emulator**

```bash
adb -s emulator-5554 install -r apps/android/app/build/outputs/apk/debug/app-debug.apk
```
Expected: `Success`

- [ ] **Step 3: Launch app**

```bash
adb -s emulator-5554 shell am start -n com.axon.app/.MainActivity
sleep 2
```

- [ ] **Step 4: Test FAB ring — Back key dismissal**

```bash
# Open FAB ring
adb -s emulator-5554 shell input tap 983 2135
sleep 1
# Press Back — must dismiss ring, not exit app
adb -s emulator-5554 shell input keyevent KEYCODE_BACK
sleep 0.5
# Verify app still in foreground (should show Axon home, not system launcher)
adb -s emulator-5554 shell dumpsys window windows | grep mCurrentFocus
```
Expected output contains: `com.axon.app`

- [ ] **Step 5: Test FAB ring — all 10 ops visible**

```bash
# Open FAB ring
adb -s emulator-5554 shell input tap 983 2135
sleep 1.5
adb -s emulator-5554 shell uiautomator dump /sdcard/ui_ring.xml && \
  adb -s emulator-5554 pull /sdcard/ui_ring.xml /tmp/ui_ring_p3.xml
python3 -c "
import xml.etree.ElementTree as ET
ops = ['Scrape','Research','Extract','Query','Search','Map','Retrieve','Summarize','Crawl','Ingest']
tree = ET.parse('/tmp/ui_ring_p3.xml')
found = [n.get('content-desc','') for n in tree.iter() if n.get('content-desc','') in ops]
print('Found:', found)
print('Missing:', [o for o in ops if o not in found])
"
```
Expected: `Missing: []` (all 10 ops found in UI dump)

- [ ] **Step 6: Dismiss ring with backdrop tap**

```bash
# Ring should still be open — tap center of screen to dismiss
adb -s emulator-5554 shell input tap 540 1200
sleep 0.5
```

- [ ] **Step 7: Test MGMT drawer — sub-items visible**

```bash
# Tap MGMT rail item (bounds [9,476][135,602], center 72,539)
adb -s emulator-5554 shell input tap 72 539
sleep 0.8
adb -s emulator-5554 shell uiautomator dump /sdcard/ui_mgmt.xml && \
  adb -s emulator-5554 pull /sdcard/ui_mgmt.xml /tmp/ui_mgmt_p3.xml
python3 -c "
import xml.etree.ElementTree as ET
tree = ET.parse('/tmp/ui_mgmt_p3.xml')
texts = [n.get('text','') for n in tree.iter() if n.get('text','') in ['Monitor','Stack','Dedupe','Sync','Config']]
print('Found MGMT items:', texts)
"
```
Expected: `Found MGMT items:` containing all five labels

- [ ] **Step 8: Tap Config — verify SettingsRoute navigates**

```bash
# Config is the last MGMT sub-item — tap its approximate bounds
adb -s emulator-5554 shell uiautomator dump /sdcard/ui_mgmt2.xml && \
  adb -s emulator-5554 pull /sdcard/ui_mgmt2.xml /tmp/ui_mgmt2.xml
python3 -c "
import xml.etree.ElementTree as ET, re
tree = ET.parse('/tmp/ui_mgmt2.xml')
for n in tree.iter():
    if n.get('text') == 'Config':
        m = re.match(r'\[(\d+),(\d+)\]\[(\d+),(\d+)\]', n.get('bounds',''))
        if m:
            x1,y1,x2,y2 = map(int,m.groups())
            print(f'{(x1+x2)//2} {(y1+y2)//2}')
" | xargs -I{} sh -c 'read x y <<< "{}"; adb -s emulator-5554 shell input tap \$x \$y'
sleep 1
# Check current screen title
adb -s emulator-5554 shell uiautomator dump /sdcard/ui_settings.xml && \
  adb -s emulator-5554 pull /sdcard/ui_settings.xml /tmp/ui_settings.xml
grep -o 'text="Settings"' /tmp/ui_settings.xml | head -1
```
Expected: `text="Settings"`

- [ ] **Step 9: Navigate back, test SETUP drawer**

```bash
adb -s emulator-5554 shell input keyevent KEYCODE_BACK
sleep 0.5
# Dismiss MGMT drawer
adb -s emulator-5554 shell input keyevent KEYCODE_BACK
sleep 0.5
# Tap SETUP rail item (bounds [9,2256][135,2382], center 72,2319)
adb -s emulator-5554 shell input tap 72 2319
sleep 0.8
adb -s emulator-5554 shell uiautomator dump /sdcard/ui_setup.xml && \
  adb -s emulator-5554 pull /sdcard/ui_setup.xml /tmp/ui_setup_p3.xml
python3 -c "
import xml.etree.ElementTree as ET
tree = ET.parse('/tmp/ui_setup_p3.xml')
texts = [n.get('text','') for n in tree.iter() if n.get('text','') in ['Preflight','Setup','Smoke','Doctor','Debug']]
print('Found SETUP items:', texts)
"
```
Expected: `Found SETUP items:` containing all five labels

- [ ] **Step 10: Tap Smoke — verify healthz result appears**

```bash
# Tap "Smoke" sub-item
python3 -c "
import xml.etree.ElementTree as ET, re
tree = ET.parse('/tmp/ui_setup_p3.xml')
for n in tree.iter():
    if n.get('text') == 'Smoke':
        m = re.match(r'\[(\d+),(\d+)\]\[(\d+),(\d+)\]', n.get('bounds',''))
        if m:
            x1,y1,x2,y2 = map(int,m.groups())
            print(f'{(x1+x2)//2} {(y1+y2)//2}')
" | read x y; adb -s emulator-5554 shell input tap $x $y
sleep 3
adb -s emulator-5554 shell uiautomator dump /sdcard/ui_setup2.xml && \
  adb -s emulator-5554 pull /sdcard/ui_setup2.xml /tmp/ui_setup2.xml
grep -o 'text=".*healthz.*"' /tmp/ui_setup2.xml | head -1
```
Expected: line containing `/healthz → 200 OK` or an error message (not "Tap to run")

- [ ] **Step 11: Commit version bump**

Check current version in `Cargo.toml` and bump patch for these fixes:
```bash
grep '^version' /home/jmagar/workspace/axon_rust/apps/android/app/build.gradle.kts | head -1
```
Then bump `versionCode` and `versionName` in `app/build.gradle.kts` (minor bump for new features):

```bash
# Verify build.gradle.kts version line, then edit manually or via sed
grep -n "versionCode\|versionName" apps/android/app/build.gradle.kts
```

- [ ] **Step 12: Final commit and push**

```bash
git add apps/android/
git commit -m "feat(android): Phase 3 — FAB fixes + MGMT/SETUP drawers wired"
rtk git push
```

---

## Self-Review

### Spec coverage

| Spec requirement | Task |
|---|---|
| FAB ring — Back key dismisses ring | Task 1 |
| FAB ring — all 10 ops visible (no clipping) | Task 2 |
| Management drawer — 5 sub-items per spec | Tasks 3–4, 7 |
| Setup drawer — 5 sub-items per spec | Tasks 5–6, 7 |
| Dim backdrop when ring is open | Task 2b |
| Config sub-item → navigates to Settings | Tasks 4, 7 |
| Setup sub-item → navigates to Settings | Tasks 6, 7 |
| Smoke test wired to `/healthz` | Task 5–6 |
| Doctor test wired to `/v1/doctor` | Tasks 3–6 |

**Gaps:** Dedupe and Sync in Management are marked "coming soon" — no server endpoint exists yet. This is intentional: the plan defers these until the server routes are added.

### Placeholder scan

- No "TBD" or "TODO" in code blocks ✓
- All type names consistent across tasks (`MgmtActionState`, `SetupActionState`) ✓
- `onOpenSettings: () -> Unit` signature matches Task 7 usage ✓
- `ManagementViewModel` property `statsState`/`doctorState` match Task 4 collectors ✓
- `SetupViewModel` property `smokeState`/`doctorState` match Task 6 collectors ✓

### Type consistency

- `MgmtActionState` defined in Task 3, used in Task 4 — consistent ✓
- `SetupActionState` defined in Task 5, used in Task 6 — consistent ✓
- `ConnectionStatusViewModel.refresh()` used in Task 4 — exists in `ConnectionStatusViewModel.kt` ✓
- `AxonClient.healthz()`, `.doctor()`, `.stats()` — all present in `AxonClient.kt` ✓
- `SettingsRoute` — defined in `AxonNavGraph.kt` same package, no import needed ✓
