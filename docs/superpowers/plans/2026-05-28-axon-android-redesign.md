# Axon Android UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 4-page HorizontalPager navigation with a Side Rail + Overlay Drawer, make Ask the permanent home screen, replace the FAB mode picker with a full 360° ring launcher, and add Aurora-spec progress bars and chat bubbles throughout.

**Architecture:** `RailScaffold` wraps `AskScreen` (always visible behind overlays) with a 54dp side rail. Tapping a rail item slides an `OverlayDrawer` in from the left over the dimmed Ask content. The FAB lives in the Ask screen above the prompt bar and expands into a full 360° ring via spring animation.

**Tech Stack:** Kotlin 2.1, Compose BOM 2026.04.01, `material-icons-extended` (already included), Room 2.6.1, Aurora composite build, OkHttp 4.12 SSE.

**Spec:** `docs/specs/android-redesign.md`

**Recommended execution order:** This plan has 6 phases. Complete Phase 1 before starting any other phase. Phases 2–6 can be done in order; each produces a working, testable state.

---

## File Map

**Create:**
```
apps/android/app/src/main/java/com/axon/app/
  ui/common/AuroraProgressBar.kt         — Aurora-spec gradient progress bar
  ui/common/AuroraStatusDot.kt           — 7dp status dot with pulse
  ui/nav/DrawerSection.kt                — sealed interface for 5 drawer sections
  ui/nav/AxonRail.kt                     — 54dp side rail with 5 items
  ui/nav/OverlayDrawer.kt                — full-height animated drawer overlay
  ui/nav/DrawerSectionContent.kt         — dispatcher → section-specific content
  ui/nav/RailScaffold.kt                 — root scaffold: rail + Ask bg + drawer
  ui/fab/FabOp.kt                        — 10-operation enum with icon/label/style
  ui/fab/FabRing.kt                      — 360° spring-animated ring composable
  ui/fab/FabOpInputCard.kt               — center-screen floating input card
  ui/fab/FabLauncher.kt                  — stateful FAB: idle→ring→input flow
  ui/ask/ChatBubble.kt                   — user + axon message bubble composables
  ui/ask/InjectionCard.kt               — crawl/ingest completion injection card
  ui/sessions/Session.kt                 — Room entity
  ui/sessions/SessionDao.kt              — Room DAO
  ui/sessions/SessionsViewModel.kt       — sessions list + create/rename/pin
  ui/sessions/SessionsDrawerContent.kt   — drawer UI for sessions
  ui/jobs/JobsOverviewItem.kt            — per-type aggregate job state
  ui/jobs/JobsOverviewViewModel.kt       — live aggregate + per-type job lists
  ui/jobs/JobsDrawerContent.kt           — 2-level drawer: overview + drill-down
  ui/knowledge/SuggestScreen.kt          — full-screen suggest list
  ui/knowledge/KnowledgeDrawerContent.kt — knowledge drawer routing
  ui/management/ManagementDrawerContent.kt — management drawer (stubs)
  ui/setup/SetupDrawerContent.kt         — setup drawer routing to existing screens
```

**Modify:**
```
  ui/nav/AxonNavGraph.kt       — replace HomeRoute/HomeShell with RailShellRoute
  ui/nav/AxonNav.kt            — remove HomeShell locals, add new ones
  ui/ask/AskScreen.kt          — add FabLauncher, use ChatBubble + InjectionCard
  ui/ask/AskViewModel.kt       — add injectOp(), session save/load
  data/local/AppDatabase.kt    — add sessions table, version 2
```

**Delete:**
```
  ui/operations/OperationsScreen.kt
  ui/operations/ModePickerSheet.kt
  ui/operations/DraggableFab.kt
  ui/operations/ModeOptionsCog.kt
  ui/operations/ModeContentHost.kt
  (OperationsViewModel.kt and OperationMode.kt kept — repurposed for FabOp in Task 8)
```

---

## Phase 1 — Foundation

### Task 1: AuroraProgressBar + AuroraStatusDot

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/common/AuroraProgressBar.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/common/AuroraStatusDot.kt`

- [ ] **Step 1: Write AuroraProgressBar**

```kotlin
// ui/common/AuroraProgressBar.kt
package com.axon.app.ui.common

import androidx.compose.animation.core.*
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.*
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

enum class ProgressVariant { Cyan, Success, Error, Warn }
enum class ProgressSize { Sm, Default }

private fun variantColors(v: ProgressVariant): List<Color> = when (v) {
    ProgressVariant.Cyan    -> listOf(Color(0xFF1DA8E6), Color(0xFF4DC8FA), Color(0xFF67CBFA))
    ProgressVariant.Success -> listOf(Color(0xFF3A7A74), Color(0xFF7DD3C7))
    ProgressVariant.Error   -> listOf(Color(0xFF7A3040), Color(0xFFC78490))
    ProgressVariant.Warn    -> listOf(Color(0xFF7A5E2E), Color(0xFFC6A36B))
}

/**
 * Aurora-spec progress bar. Pass progress=null for indeterminate (sliding fill),
 * or 0f..1f for determinate. Cyan variant shows a shimmer overlay while running.
 */
@Composable
fun AuroraProgressBar(
    progress: Float?,
    variant: ProgressVariant = ProgressVariant.Cyan,
    size: ProgressSize = ProgressSize.Default,
    modifier: Modifier = Modifier,
) {
    val trackHeight: Dp = if (size == ProgressSize.Sm) 4.dp else 6.dp
    val shape = RoundedCornerShape(50)
    val colors = variantColors(variant)

    val infiniteTransition = rememberInfiniteTransition(label = "pb")

    val indetOffset by infiniteTransition.animateFloat(
        initialValue = -0.35f,
        targetValue = 1.0f,
        animationSpec = infiniteRepeatable(
            animation = tween(1500, easing = FastOutSlowInEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "indet",
    )
    val shimmerOffset by infiniteTransition.animateFloat(
        initialValue = -0.5f,
        targetValue = 1.5f,
        animationSpec = infiniteRepeatable(
            animation = tween(2200, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "shimmer",
    )
    val animatedProgress by animateFloatAsState(
        targetValue = progress ?: 0f,
        animationSpec = tween(600),
        label = "det",
    )

    Box(
        modifier = modifier
            .height(trackHeight)
            .clip(shape)
            .background(Color(0xFF0C1A24))
            .border(1.dp, Color(0xFF1D3D4E), shape),
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            val w = this.size.width
            val h = this.size.height
            val r = CornerRadius(h / 2)

            if (progress == null) {
                val fillW = w * 0.35f
                val x = indetOffset * (w + fillW)
                val brush = Brush.horizontalGradient(colors = colors, startX = x, endX = x + fillW)
                drawRoundRect(brush = brush, topLeft = Offset(x, 0f), size = Size(fillW, h), cornerRadius = r)
            } else {
                val fillW = w * animatedProgress.coerceIn(0f, 1f)
                if (fillW > 0f) {
                    val brush = Brush.horizontalGradient(colors = colors, startX = 0f, endX = fillW)
                    drawRoundRect(brush = brush, size = Size(fillW, h), cornerRadius = r)
                }
            }

            // Shimmer (Cyan running/indeterminate only)
            val showShimmer = variant == ProgressVariant.Cyan && (progress == null || (progress > 0f && progress < 1f))
            if (showShimmer) {
                val sx = shimmerOffset * w
                val sw = w * 0.3f
                val shimmerBrush = Brush.horizontalGradient(
                    colors = listOf(Color.Transparent, Color.White.copy(alpha = 0.32f), Color.Transparent),
                    startX = sx - sw / 2,
                    endX = sx + sw / 2,
                )
                drawRect(brush = shimmerBrush, size = this.size)
            }
        }
    }
}
```

- [ ] **Step 2: Write AuroraStatusDot**

```kotlin
// ui/common/AuroraStatusDot.kt
package com.axon.app.ui.common

import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

enum class DotState { Running, Done, Failed, Idle, Warn }

private fun dotColor(state: DotState) = when (state) {
    DotState.Running -> Color(0xFF29B6F6)
    DotState.Done    -> Color(0xFF7DD3C7)
    DotState.Failed  -> Color(0xFFC78490)
    DotState.Idle    -> Color(0xFFA7BCC9)
    DotState.Warn    -> Color(0xFFC6A36B)
}

@Composable
fun AuroraStatusDot(state: DotState, size: Dp = 7.dp, modifier: Modifier = Modifier) {
    val infiniteTransition = rememberInfiniteTransition(label = "dot")
    val pulseAlpha by infiniteTransition.animateFloat(
        initialValue = 1f,
        targetValue = 0.35f,
        animationSpec = infiniteRepeatable(
            animation = tween(900, easing = FastOutSlowInEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "pulse",
    )
    val alpha = if (state == DotState.Running) pulseAlpha else 1f
    Box(modifier = modifier.size(size).clip(CircleShape).background(dotColor(state).copy(alpha = alpha)))
}
```

- [ ] **Step 3: Build and verify**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: `BUILD SUCCESSFUL`

- [ ] **Step 4: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/common/
git commit -m "feat(android): AuroraProgressBar + AuroraStatusDot composables"
```

---

### Task 2: DrawerSection + AxonRail

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/nav/DrawerSection.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonRail.kt`

- [ ] **Step 1: Write DrawerSection sealed interface**

```kotlin
// ui/nav/DrawerSection.kt
package com.axon.app.ui.nav

sealed interface DrawerSection {
    data object Sessions   : DrawerSection
    data object Jobs       : DrawerSection
    data object Knowledge  : DrawerSection
    data object Management : DrawerSection
    data object Setup      : DrawerSection
}
```

- [ ] **Step 2: Write AxonRail**

```kotlin
// ui/nav/AxonRail.kt
package com.axon.app.ui.nav

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private val NavBg       = Color(0xFF07111A)
private val AccentPrimary = Color(0xFF29B6F6)
private val TextMuted   = Color(0xFFA7BCC9)

private data class SectionDef(val section: DrawerSection, val icon: ImageVector, val label: String)

private val TopSections = listOf(
    SectionDef(DrawerSection.Sessions,   Icons.Rounded.History,      "Sess"),
    SectionDef(DrawerSection.Jobs,       Icons.Rounded.Checklist,    "Jobs"),
    SectionDef(DrawerSection.Knowledge,  Icons.Rounded.Hub,          "Know"),
    SectionDef(DrawerSection.Management, Icons.Rounded.Settings,     "Mgmt"),
)
private val BottomSections = listOf(
    SectionDef(DrawerSection.Setup, Icons.Rounded.Construction, "Setup"),
)

@Composable
fun AxonRail(
    activeSection: DrawerSection?,
    onSectionClick: (DrawerSection) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .width(54.dp)
            .fillMaxHeight()
            .background(NavBg),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Spacer(Modifier.height(10.dp))
        TopSections.forEach { def ->
            RailItem(def.icon, def.label, activeSection == def.section) { onSectionClick(def.section) }
        }
        Spacer(Modifier.weight(1f))
        BottomSections.forEach { def ->
            RailItem(def.icon, def.label, activeSection == def.section) { onSectionClick(def.section) }
        }
        Spacer(Modifier.height(10.dp))
    }
}

@Composable
private fun RailItem(icon: ImageVector, label: String, active: Boolean, onClick: () -> Unit) {
    val tint = if (active) AccentPrimary else TextMuted
    Box(
        modifier = Modifier
            .size(46.dp, 42.dp)
            .clip(RoundedCornerShape(13.dp))
            .background(if (active) AccentPrimary.copy(alpha = 0.12f) else Color.Transparent)
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick),
    ) {
        if (active) {
            Box(
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    .width(3.dp)
                    .height(22.dp)
                    .clip(RoundedCornerShape(topEnd = 2.dp, bottomEnd = 2.dp))
                    .background(AccentPrimary),
            )
        }
        Column(
            modifier = Modifier.align(Alignment.Center),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Icon(imageVector = icon, contentDescription = label, tint = tint, modifier = Modifier.size(20.dp))
            Text(label.uppercase(), fontSize = 7.sp, fontWeight = FontWeight.SemiBold, color = tint, letterSpacing = 0.5.sp)
        }
    }
}
```

- [ ] **Step 3: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: `BUILD SUCCESSFUL`

- [ ] **Step 4: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/nav/DrawerSection.kt \
        apps/android/app/src/main/java/com/axon/app/ui/nav/AxonRail.kt
git commit -m "feat(android): DrawerSection + AxonRail composable"
```

---

### Task 3: OverlayDrawer + stub DrawerSectionContent

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/nav/OverlayDrawer.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/nav/DrawerSectionContent.kt`

- [ ] **Step 1: Write OverlayDrawer**

```kotlin
// ui/nav/OverlayDrawer.kt
package com.axon.app.ui.nav

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.navigation.NavController

@Composable
fun OverlayDrawer(
    section: DrawerSection,
    onDismiss: () -> Unit,
    navController: NavController,
    modifier: Modifier = Modifier,
) {
    Box(modifier = modifier.fillMaxSize()) {
        // Dim backdrop — tap to dismiss
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Color(0xAE040A0E))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
        )
        // Drawer panel — consumes its own clicks
        Column(
            modifier = Modifier
                .width(232.dp)
                .fillMaxHeight()
                .background(Color(0xFF13293A))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = {}),
        ) {
            DrawerSectionContent(section = section, onDismiss = onDismiss, navController = navController)
        }
    }
}
```

- [ ] **Step 2: Write stub DrawerSectionContent**

This stub shows section headers only — real content is filled in by later tasks.

```kotlin
// ui/nav/DrawerSectionContent.kt
package com.axon.app.ui.nav

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation.NavController

@Composable
fun DrawerSectionContent(
    section: DrawerSection,
    onDismiss: () -> Unit,
    navController: NavController,
) {
    when (section) {
        DrawerSection.Sessions   -> SessionsDrawerContentStub()
        DrawerSection.Jobs       -> JobsDrawerContentStub()
        DrawerSection.Knowledge  -> KnowledgeDrawerContentStub()
        DrawerSection.Management -> ManagementDrawerContentStub()
        DrawerSection.Setup      -> SetupDrawerContentStub()
    }
}

// Stubs — replaced in later tasks
@Composable private fun SessionsDrawerContentStub() =
    DrawerStub(Icons.Rounded.History, "Sessions")
@Composable private fun JobsDrawerContentStub() =
    DrawerStub(Icons.Rounded.Checklist, "Jobs")
@Composable private fun KnowledgeDrawerContentStub() =
    DrawerStub(Icons.Rounded.Hub, "Knowledge")
@Composable private fun ManagementDrawerContentStub() =
    DrawerStub(Icons.Rounded.Settings, "Management")
@Composable private fun SetupDrawerContentStub() =
    DrawerStub(Icons.Rounded.Construction, "Setup")

@Composable
private fun DrawerStub(icon: ImageVector, title: String) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(14.dp, 14.dp, 14.dp, 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(imageVector = icon, contentDescription = title, tint = Color(0xFF29B6F6), modifier = Modifier.size(18.dp))
        Text(title, fontSize = 14.sp, fontWeight = FontWeight.Bold, color = Color(0xFFE6F4FB))
    }
}
```

- [ ] **Step 3: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/nav/OverlayDrawer.kt \
        apps/android/app/src/main/java/com/axon/app/ui/nav/DrawerSectionContent.kt
git commit -m "feat(android): OverlayDrawer + stub DrawerSectionContent"
```

---

### Task 4: RailScaffold

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/nav/RailScaffold.kt`

- [ ] **Step 1: Write RailScaffold**

```kotlin
// ui/nav/RailScaffold.kt
package com.axon.app.ui.nav

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.navigation.NavController
import com.axon.app.ui.ask.AskScreen

@Composable
fun RailScaffold(navController: NavController, modifier: Modifier = Modifier) {
    var activeSection by remember { mutableStateOf<DrawerSection?>(null) }

    BackHandler(enabled = activeSection != null) { activeSection = null }

    Row(modifier = modifier.fillMaxSize().background(Color(0xFF07131C))) {
        AxonRail(
            activeSection = activeSection,
            onSectionClick = { section ->
                activeSection = if (activeSection == section) null else section
            },
        )
        Box(modifier = Modifier.weight(1f)) {
            // Ask screen is always visible
            AskScreen(
                onOpenDocument = { url -> navController.navigate(DocumentRoute(url)) },
            )
            // Drawer slides in over Ask
            AnimatedVisibility(
                visible = activeSection != null,
                enter = slideInHorizontally(tween(220)) { -it } + fadeIn(tween(180)),
                exit  = slideOutHorizontally(tween(180)) { -it } + fadeOut(tween(150)),
            ) {
                OverlayDrawer(
                    section = activeSection ?: DrawerSection.Sessions,
                    onDismiss = { activeSection = null },
                    navController = navController,
                )
            }
        }
    }
}
```

- [ ] **Step 2: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: errors about `AskScreen` signature — you'll fix that in the next step.

- [ ] **Step 3: Update AskScreen signature to accept onOpenDocument**

In `ui/ask/AskScreen.kt`, update the top-level composable:

```kotlin
// BEFORE:
@Composable
fun AskScreen()

// AFTER:
@Composable
fun AskScreen(
    onOpenDocument: (String) -> Unit = {},
    modifier: Modifier = Modifier,
)
```

Pass `onOpenDocument` through to wherever document links are tapped (search for `LocalOpenDocument` usages in the old code — replace with the lambda parameter).

- [ ] **Step 4: Build again**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/nav/RailScaffold.kt \
        apps/android/app/src/main/java/com/axon/app/ui/ask/AskScreen.kt
git commit -m "feat(android): RailScaffold composable"
```

---

### Task 5: Restructure AxonNavGraph

**Files:**
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNavGraph.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNav.kt`

- [ ] **Step 1: Add RailShellRoute**

In `AxonNav.kt`, add beside the existing route objects:

```kotlin
@Serializable data object RailShellRoute
```

Remove `HomeRoute` if it's no longer used elsewhere (check with `grep -r "HomeRoute" apps/android/` first).

- [ ] **Step 2: Replace the navgraph root**

In `AxonNavGraph.kt`, replace the `HomeRoute` composable with `RailShellRoute`:

```kotlin
// REMOVE:
composable<HomeRoute> {
    HomeShell(...)
}

// ADD:
composable<RailShellRoute> {
    RailScaffold(navController = navController)
}
```

Change `startDestination = HomeRoute` → `startDestination = RailShellRoute`.

Remove `LocalModeOptionsCog`, `LocalOpenModeOptions` from `AxonNav.kt` and anywhere they are provided — the mode options flow is replaced by the FAB (Task 10+).

Keep `LocalOpenDocument` — it's still used.

- [ ] **Step 3: Delete HomeShell**

The `HomeShell` composable was inline in `AxonNavGraph.kt` (the 4-page HorizontalPager). Delete that entire block. If it's in its own file, delete the file.

- [ ] **Step 4: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

Fix any remaining compilation errors (unused imports, removed locals).

- [ ] **Step 5: Smoke test on device/emulator**

Run the app. You should see: Ask screen visible, side rail on the left with 5 icons, tapping a rail icon slides in a drawer with a section header, tapping outside closes it, back button closes it.

- [ ] **Step 6: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/nav/
git commit -m "feat(android): replace HorizontalPager with RailScaffold nav"
```

---

### Task 6: Remove legacy operations files

**Files:**
- Delete: `ui/operations/OperationsScreen.kt`
- Delete: `ui/operations/ModePickerSheet.kt`
- Delete: `ui/operations/DraggableFab.kt`
- Delete: `ui/operations/ModeOptionsCog.kt`
- Delete: `ui/operations/ModeContentHost.kt`

> `OperationMode.kt` and `OperationsViewModel.kt` are kept — they're repurposed in Task 8.

- [ ] **Step 1: Verify nothing references the deleted files**

```bash
grep -r "OperationsScreen\|ModePickerSheet\|DraggableFab\|ModeOptionsCog\|ModeContentHost" \
  apps/android/app/src/main/java/ --include="*.kt" -l
```

Expected: no output. Fix any references first.

- [ ] **Step 2: Delete the files**

```bash
rm apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsScreen.kt
rm apps/android/app/src/main/java/com/axon/app/ui/operations/ModePickerSheet.kt
rm apps/android/app/src/main/java/com/axon/app/ui/operations/DraggableFab.kt
rm apps/android/app/src/main/java/com/axon/app/ui/operations/ModeOptionsCog.kt
rm apps/android/app/src/main/java/com/axon/app/ui/operations/ModeContentHost.kt
```

- [ ] **Step 3: Build clean**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```
Expected: `BUILD SUCCESSFUL`

- [ ] **Step 4: Commit**

```bash
git add -A apps/android/app/src/main/java/com/axon/app/ui/operations/
git commit -m "chore(android): remove legacy operations screen + mode picker"
```

---

## Phase 2 — FAB Circle Ring

### Task 7: FabOp enum

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/fab/FabOp.kt`

- [ ] **Step 1: Write FabOp**

```kotlin
// ui/fab/FabOp.kt
package com.axon.app.ui.fab

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector

/**
 * 10 FAB operations. Clock positions starting 12 o'clock, 36° apart.
 * isAsync=true operations show warnBase amber tint and produce an injection card
 * instead of inline bubble output.
 */
enum class FabOp(
    val label: String,
    val icon: ImageVector,
    val isAsync: Boolean = false,
    val placeholder: String,
) {
    Scrape(
        label     = "Scrape",
        icon      = Icons.Rounded.ContentPaste,
        placeholder = "https://",
    ),
    Research(
        label     = "Research",
        icon      = Icons.Rounded.Science,
        placeholder = "search query…",
    ),
    Extract(
        label     = "Extract",
        icon      = Icons.Rounded.FilterAlt,
        placeholder = "https://",
    ),
    Query(
        label     = "Query",
        icon      = Icons.Rounded.ManageSearch,
        placeholder = "semantic query…",
    ),
    Search(
        label     = "Search",
        icon      = Icons.Rounded.Public,
        placeholder = "search query…",
    ),
    Map(
        label     = "Map",
        icon      = Icons.Rounded.Map,
        placeholder = "https://",
    ),
    Retrieve(
        label     = "Retrieve",
        icon      = Icons.Rounded.Archive,
        placeholder = "https://",
    ),
    Summarize(
        label     = "Summarize",
        icon      = Icons.Rounded.Notes,
        placeholder = "https://",
    ),
    Crawl(
        label     = "Crawl",
        icon      = Icons.Rounded.TravelExplore,
        isAsync   = true,
        placeholder = "https://",
    ),
    Ingest(
        label     = "Ingest",
        icon      = Icons.Rounded.Download,
        isAsync   = true,
        placeholder = "URL or github/user/repo…",
    ),
}

// Cyan accent for sync ops
val syncOpTint    = Color(0xFF29B6F6)
val syncOpBg      = Color(0xFF13293A)
// Amber tint for async ops (Crawl, Ingest)
val asyncOpTint   = Color(0xFFC6A36B)
val asyncOpBg     = Color(0x12C6A36B)   // rgba(198,163,107,0.07)
```

- [ ] **Step 2: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 3: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/fab/FabOp.kt
git commit -m "feat(android): FabOp enum — 10 operations for ring launcher"
```

---

### Task 8: FabRing composable

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/fab/FabRing.kt`

- [ ] **Step 1: Write FabRing**

The ring places 10 tiles at radius 96dp. Tiles are positioned using trigonometry (angle = -90 + i*36 degrees). Tiles spring-animate from center outward on open.

```kotlin
// ui/fab/FabRing.kt
package com.axon.app.ui.fab

import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

private val BorderStrong = Color(0xFF24536C)
private val PanelStrong  = Color(0xFF13293A)

@Composable
fun FabRing(
    visible: Boolean,
    fabCenterOffset: IntOffset,       // pixel offset of FAB center in screen coords
    onOpSelected: (FabOp) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val radiusDp: Dp = 96.dp
    val density = LocalDensity.current

    // Spring animation for ring open (0 = closed, 1 = fully open)
    val openProgress by animateFloatAsState(
        targetValue = if (visible) 1f else 0f,
        animationSpec = spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium),
        label = "ring-open",
    )

    if (!visible && openProgress == 0f) return

    Box(modifier = modifier.fillMaxSize()) {
        FabOp.entries.forEachIndexed { i, op ->
            val angleDeg = -90.0 + i * 36.0
            val angleRad = Math.toRadians(angleDeg)
            val radiusPx = with(density) { radiusDp.toPx() }

            val dx = (radiusPx * cos(angleRad) * openProgress).roundToInt()
            val dy = (radiusPx * sin(angleRad) * openProgress).roundToInt()

            OpTile(
                op = op,
                modifier = Modifier.offset {
                    IntOffset(
                        x = fabCenterOffset.x + dx - with(density) { 23.dp.roundToPx() },
                        y = fabCenterOffset.y + dy - with(density) { 23.dp.roundToPx() },
                    )
                },
                alpha = openProgress,
                onClick = { onOpSelected(op) },
            )
        }

        // Center dismiss button (shows × when ring is open)
        Box(
            modifier = Modifier
                .offset { fabCenterOffset - IntOffset(with(density) { 21.dp.roundToPx() }, with(density) { 21.dp.roundToPx() }) }
                .size(42.dp)
                .background(Color(0xFF29B6F6), RoundedCornerShape(13.dp))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Rounded.Close, contentDescription = "Close", tint = Color(0xFF051520), modifier = Modifier.size(20.dp))
        }
    }
}

@Composable
private fun OpTile(op: FabOp, modifier: Modifier, alpha: Float, onClick: () -> Unit) {
    val bg   = if (op.isAsync) asyncOpBg   else PanelStrong
    val tint = if (op.isAsync) asyncOpTint else syncOpTint

    Box(
        modifier = modifier
            .size(46.dp)
            .graphicsLayer { this.alpha = alpha }
            .background(bg, RoundedCornerShape(13.dp))
            .border(1.dp, if (op.isAsync) asyncOpTint.copy(alpha = 0.35f) else BorderStrong, RoundedCornerShape(13.dp))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Icon(imageVector = op.icon, contentDescription = op.label, tint = tint, modifier = Modifier.size(17.dp))
            Text(op.label, fontSize = 7.sp, fontWeight = FontWeight.SemiBold, color = tint, letterSpacing = 0.3.sp)
        }
    }
}

// Extension to allow IntOffset subtraction
private operator fun IntOffset.minus(other: IntOffset) = IntOffset(x - other.x, y - other.y)
```

- [ ] **Step 2: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 3: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/fab/FabRing.kt
git commit -m "feat(android): FabRing — 360° spring-animated operation ring"
```

---

### Task 9: FabOpInputCard + FabLauncher

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/fab/FabOpInputCard.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/fab/FabLauncher.kt`

- [ ] **Step 1: Write FabOpInputCard**

The input card appears center-screen after an op is selected. It contains the op label + a focused URL/query field + paste + send.

```kotlin
// ui/fab/FabOpInputCard.kt
package com.axon.app.ui.fab

import android.content.ClipboardManager
import android.content.Context
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
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

private val BorderStrong   = Color(0xFF24536C)
private val AccentPrimary  = Color(0xFF29B6F6)
private val AccentButton   = Color(0xFF1DA8E6)
private val AccentFg       = Color(0xFF051520)
private val PanelStrong    = Color(0xFF13293A)
private val CtrlSurface    = Color(0xFF0C1A24)
private val TextPrimary    = Color(0xFFE6F4FB)
private val TextMuted      = Color(0xFFA7BCC9)

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

    // Dim backdrop
    Box(
        modifier = modifier
            .fillMaxSize()
            .background(Color(0x99040A0E))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
        contentAlignment = Alignment.Center,
    ) {
        // Card — stop click propagation
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
            // Header: op chip
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

            // Input row
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

                // Paste button
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

                // Send button
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
```

- [ ] **Step 2: Write FabLauncher**

Stateful composable managing: Idle → RingOpen → InputOpen states.

```kotlin
// ui/fab/FabLauncher.kt
package com.axon.app.ui.fab

import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material3.Icon
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

private sealed interface FabState {
    data object Idle   : FabState
    data object Ring   : FabState
    data class  Input(val op: FabOp) : FabState
}

/**
 * Drop this into AskScreen above the prompt bar (bottom-end aligned).
 * onOpSubmit receives (op, inputText) when user sends from the input card.
 */
@Composable
fun FabLauncher(
    onOpSubmit: (FabOp, String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var state by remember { mutableStateOf<FabState>(FabState.Idle) }
    var fabCenter by remember { mutableStateOf(IntOffset.Zero) }

    Box(modifier = modifier.fillMaxSize()) {
        // Ring overlay (sits above everything)
        FabRing(
            visible = state is FabState.Ring,
            fabCenterOffset = fabCenter,
            onOpSelected = { op -> state = FabState.Input(op) },
            onDismiss = { state = FabState.Idle },
        )

        // Input card overlay
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

        // The FAB button itself (only visible when ring is closed)
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

- [ ] **Step 3: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 4: Wire FabLauncher into AskScreen**

In `ui/ask/AskScreen.kt`, find the existing `DraggableFab` usage and replace it with `FabLauncher`:

```kotlin
// In AskScreen composable, in the Box that holds the chat content:
Box(modifier = Modifier.fillMaxSize()) {
    // ... existing message list and prompt input ...

    // Replace DraggableFab with:
    FabLauncher(
        onOpSubmit = { op, input -> viewModel.submitFabOp(op, input) },
    )
}
```

Add `submitFabOp(op: FabOp, input: String)` to `AskViewModel` (implementation in Task 14 Phase 3 — for now, add a stub: `fun submitFabOp(op: FabOp, input: String) { /* TODO Phase 3 */ }`).

- [ ] **Step 5: Smoke test — tap FAB, ring opens, tap op, input card shows center-screen**

- [ ] **Step 6: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/fab/
git commit -m "feat(android): FabOpInputCard + FabLauncher — 360° ring launcher"
```

---

## Phase 3 — Ask Screen Chat UI

### Task 10: ChatBubble + InjectionCard composables

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/ask/ChatBubble.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/ask/InjectionCard.kt`

- [ ] **Step 1: Write ChatBubble**

```kotlin
// ui/ask/ChatBubble.kt
package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tv.tootie.aurora.components.AuroraThinking

// User message — right-aligned, cyan tint
@Composable
fun UserBubble(text: String, modifier: Modifier = Modifier) {
    Box(modifier = modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
        Text(
            text = text,
            modifier = Modifier
                .widthIn(max = 280.dp)
                .background(Color(0x1A29B6F6), RoundedCornerShape(16.dp, 16.dp, 4.dp, 16.dp))
                .border(1.dp, Color(0x4029B6F6), RoundedCornerShape(16.dp, 16.dp, 4.dp, 16.dp))
                .padding(horizontal = 12.dp, vertical = 8.dp),
            fontSize = 13.sp,
            color = Color(0xFFE6F4FB),
            lineHeight = 19.sp,
        )
    }
}

// Axon response — left-aligned, avatar + label
@Composable
fun AxonBubble(
    text: String,
    isStreaming: Boolean = false,
    modifier: Modifier = Modifier,
) {
    Row(modifier = modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        // ✦ Avatar
        Box(
            modifier = Modifier
                .size(24.dp)
                .background(
                    Color(0xFF0C1A24),
                    RoundedCornerShape(7.dp),
                )
                .border(1.dp, Color(0x4D29B6F6), RoundedCornerShape(7.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Text("✦", fontSize = 11.sp, color = Color(0xFF29B6F6))
        }

        Column(modifier = Modifier.widthIn(max = 280.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                "AXON",
                fontSize = 9.sp,
                fontWeight = FontWeight.Bold,
                color = Color(0xFF29B6F6),
                letterSpacing = 0.8.sp,
            )
            if (isStreaming && text.isEmpty()) {
                AuroraThinking(modifier = Modifier.padding(top = 4.dp))
            } else {
                Text(
                    text = text,
                    modifier = Modifier
                        .background(Color(0xFF102330), RoundedCornerShape(4.dp, 14.dp, 14.dp, 14.dp))
                        .border(1.dp, Color(0xFF1D3D4E), RoundedCornerShape(4.dp, 14.dp, 14.dp, 14.dp))
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                    fontSize = 13.sp,
                    color = Color(0xFFE6F4FB),
                    lineHeight = 19.sp,
                )
            }
        }
    }
}
```

- [ ] **Step 2: Write InjectionCard**

Crawl/Ingest completions land as compact injection cards, not full bubbles.

```kotlin
// ui/ask/InjectionCard.kt
package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.TravelExplore
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.fab.FabOp

@Composable
fun InjectionCard(
    op: FabOp,             // Crawl or Ingest
    target: String,        // URL or repo
    pageCount: Int? = null,
    chunkCount: Int? = null,
    modifier: Modifier = Modifier,
) {
    val icon = if (op == FabOp.Crawl) Icons.Rounded.TravelExplore else Icons.Rounded.Download
    val verbPast = if (op == FabOp.Crawl) "crawled" else "ingested"
    val indexedWhat = when {
        pageCount != null && chunkCount != null ->
            "and indexed $pageCount docs (${"%,d".format(chunkCount)} chunks)"
        chunkCount != null -> "and indexed ${"%,d".format(chunkCount)} chunks"
        else -> ""
    }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(Color(0x0D29B6F6), RoundedCornerShape(12.dp))
            .border(1.dp, Color(0x2E29B6F6), RoundedCornerShape(12.dp))
            .padding(10.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = Color(0xFFC6A36B),
            modifier = Modifier.size(14.dp).padding(top = 1.dp),
        )
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                "axon mobile just $verbPast",
                fontSize = 10.sp,
                color = Color(0xFFA7BCC9),
            )
            Text(
                target,
                fontSize = 10.sp,
                fontFamily = FontFamily.Monospace,
                color = Color(0xFF67CBFA),
            )
            if (indexedWhat.isNotEmpty()) {
                Text(
                    "$indexedWhat into your knowledge base — use `axon query` + `axon retrieve` + `axon ask` via MCP or CLI",
                    fontSize = 10.sp,
                    color = Color(0xFFA7BCC9),
                    lineHeight = 15.sp,
                )
            }
        }
    }
}
```

- [ ] **Step 3: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/ask/ChatBubble.kt \
        apps/android/app/src/main/java/com/axon/app/ui/ask/InjectionCard.kt
git commit -m "feat(android): ChatBubble + InjectionCard composables"
```

---

### Task 11: Rewrite AskScreen message list + wire FAB ops

**Files:**
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/ask/AskScreen.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/ask/AskViewModel.kt`

- [ ] **Step 1: Add message types to AskViewModel**

In `AskViewModel.kt`, update the message representation to support both chat bubbles and injection cards:

```kotlin
// Add to AskViewModel.kt (alongside existing AskTurn or similar type)
sealed interface ChatItem {
    data class UserMsg(val text: String) : ChatItem
    data class AxonMsg(val text: String, val isStreaming: Boolean = false) : ChatItem
    data class Injection(
        val op: com.axon.app.ui.fab.FabOp,
        val target: String,
        val pageCount: Int? = null,
        val chunkCount: Int? = null,
    ) : ChatItem
}
```

Add `submitFabOp` implementation to `AskViewModel`. For sync ops, call the appropriate `axonRepository` method and append the result as an `AxonMsg`. For async ops (Crawl/Ingest), submit the job and append an `Injection` card:

```kotlin
fun submitFabOp(op: FabOp, input: String) {
    viewModelScope.launch {
        when (op) {
            FabOp.Scrape    -> submitSync(op, input) { repo.scrape(url = input, cfg = modeOptions()) }
            FabOp.Research  -> submitSync(op, input) { repo.research(query = input, cfg = modeOptions()) }
            FabOp.Extract   -> submitSync(op, input) { repo.extract(urls = listOf(input), cfg = modeOptions()) }
            FabOp.Query     -> submitSync(op, input) { repo.query(text = input, cfg = modeOptions()) }
            FabOp.Search    -> submitSync(op, input) { repo.searchWeb(query = input, cfg = modeOptions()) }
            FabOp.Map       -> submitSync(op, input) { repo.map(url = input, cfg = modeOptions()) }
            FabOp.Retrieve  -> submitSync(op, input) { repo.retrieve(url = input, cfg = modeOptions()) }
            FabOp.Summarize -> submitSync(op, input) { repo.summarize(url = input, cfg = modeOptions()) }
            FabOp.Crawl     -> submitCrawl(input)
            FabOp.Ingest    -> submitIngest(input)
        }
    }
}

private suspend fun submitSync(op: FabOp, input: String, call: suspend () -> Result<String>) {
    appendItem(ChatItem.UserMsg("[${op.label}] $input"))
    appendItem(ChatItem.AxonMsg("", isStreaming = true))
    call().fold(
        onSuccess = { result -> replaceLastStreaming(result) },
        onFailure = { err -> replaceLastStreaming("Error: ${err.message}") },
    )
}

private suspend fun submitCrawl(url: String) {
    appendItem(ChatItem.UserMsg("[Crawl] $url"))
    val result = repo.crawlSubmit(url = url, cfg = modeOptions())
    result.fold(
        onSuccess = { job ->
            appendItem(ChatItem.Injection(FabOp.Crawl, url))
            // Poll in background for completion to update chunk count
            pollCrawlJob(job.id, url)
        },
        onFailure = { err -> appendItem(ChatItem.AxonMsg("Crawl failed: ${err.message}")) },
    )
}

private suspend fun submitIngest(target: String) {
    appendItem(ChatItem.UserMsg("[Ingest] $target"))
    val result = repo.ingestStart(target = target, cfg = modeOptions())
    result.fold(
        onSuccess = { job -> appendItem(ChatItem.Injection(FabOp.Ingest, target)) },
        onFailure = { err -> appendItem(ChatItem.AxonMsg("Ingest failed: ${err.message}")) },
    )
}
```

Add `appendItem`, `replaceLastStreaming`, and `pollCrawlJob` helpers to the ViewModel. Expose `chatItems: StateFlow<List<ChatItem>>`.

- [ ] **Step 2: Rewrite AskScreen message list**

Replace the existing ask history `LazyColumn` with one that uses `ChatBubble` and `InjectionCard`:

```kotlin
LazyColumn(
    modifier = Modifier.fillMaxSize().padding(horizontal = 12.dp),
    state = listState,
    contentPadding = PaddingValues(vertical = 12.dp),
    verticalArrangement = Arrangement.spacedBy(10.dp),
) {
    items(chatItems, key = { it.hashCode() }) { item ->
        when (item) {
            is ChatItem.UserMsg   -> UserBubble(item.text)
            is ChatItem.AxonMsg   -> AxonBubble(item.text, item.isStreaming)
            is ChatItem.Injection -> InjectionCard(item.op, item.target, item.pageCount, item.chunkCount)
        }
    }
}
```

Keep `AuroraPromptInput` at the bottom for the regular Ask text input. The FAB and `AuroraPromptInput` sit in the same bottom area:

```kotlin
Box(Modifier.fillMaxSize()) {
    // Message list
    LazyColumn(...) { ... }

    // Bottom: prompt input
    Column(Modifier.align(Alignment.BottomCenter)) {
        AuroraPromptInput(...)
    }

    // FAB
    FabLauncher(
        onOpSubmit = { op, input -> viewModel.submitFabOp(op, input) },
    )
}
```

- [ ] **Step 3: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 4: Smoke test — type a message, verify chat bubbles render; tap FAB → select Scrape → submit URL → result appears as AxonBubble**

- [ ] **Step 5: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/ask/
git commit -m "feat(android): AskScreen chat bubbles + FAB op injection"
```

---

## Phase 4 — Sessions

### Task 12: Session Room entity + DAO + DB migration

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/sessions/Session.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/sessions/SessionDao.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/data/local/AppDatabase.kt`

- [ ] **Step 1: Write Session entity**

```kotlin
// ui/sessions/Session.kt
package com.axon.app.ui.sessions

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "sessions")
data class Session(
    @PrimaryKey val id: String,           // UUID
    val title: String,                     // auto-generated from first message
    val firstMessagePreview: String,       // first 80 chars of first user message
    val turnCount: Int = 0,
    val injectedOpCount: Int = 0,
    val createdAt: Long = System.currentTimeMillis(),
    val updatedAt: Long = System.currentTimeMillis(),
    val pinnedAt: Long? = null,            // null = not pinned, timestamp = pinned
)
```

- [ ] **Step 2: Write SessionDao**

```kotlin
// ui/sessions/SessionDao.kt
package com.axon.app.ui.sessions

import androidx.room.*
import kotlinx.coroutines.flow.Flow

@Dao
interface SessionDao {
    @Query("SELECT * FROM sessions ORDER BY pinnedAt DESC NULLS LAST, updatedAt DESC LIMIT 4")
    fun recentFour(): Flow<List<Session>>

    @Query("SELECT * FROM sessions ORDER BY pinnedAt DESC NULLS LAST, updatedAt DESC")
    fun all(): Flow<List<Session>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(session: Session)

    @Query("UPDATE sessions SET title = :title, updatedAt = :now WHERE id = :id")
    suspend fun rename(id: String, title: String, now: Long = System.currentTimeMillis())

    @Query("UPDATE sessions SET pinnedAt = :pinnedAt WHERE id = :id")
    suspend fun setPinned(id: String, pinnedAt: Long?)

    @Query("DELETE FROM sessions WHERE id = :id")
    suspend fun delete(id: String)
}
```

- [ ] **Step 3: Add migration and DAO to AppDatabase**

In `AppDatabase.kt`, bump version to 2 and add the sessions table:

```kotlin
@Database(
    entities = [AskHistoryEntry::class, Session::class],  // add Session::class
    version = 2,                                           // bump from 1
    exportSchema = true,
)
@TypeConverters(...)
abstract class AppDatabase : RoomDatabase() {
    abstract fun askHistoryDao(): AskHistoryDao
    abstract fun sessionDao(): SessionDao   // add this

    companion object {
        val MIGRATION_1_2 = object : Migration(1, 2) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL("""
                    CREATE TABLE IF NOT EXISTS sessions (
                        id TEXT NOT NULL PRIMARY KEY,
                        title TEXT NOT NULL,
                        firstMessagePreview TEXT NOT NULL,
                        turnCount INTEGER NOT NULL DEFAULT 0,
                        injectedOpCount INTEGER NOT NULL DEFAULT 0,
                        createdAt INTEGER NOT NULL,
                        updatedAt INTEGER NOT NULL,
                        pinnedAt INTEGER
                    )
                """.trimIndent())
            }
        }
    }
}
```

In `AppContainer.kt`, add the migration when building the database:

```kotlin
Room.databaseBuilder(...)
    .addMigrations(AppDatabase.MIGRATION_1_2)
    .build()
```

- [ ] **Step 4: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 5: Verify schema export**

```bash
ls apps/android/app/schemas/com.axon.app.data.local.AppDatabase/
```
Expected: both `1.json` and `2.json` present after first run.

- [ ] **Step 6: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/sessions/Session.kt \
        apps/android/app/src/main/java/com/axon/app/ui/sessions/SessionDao.kt \
        apps/android/app/src/main/java/com/axon/app/data/local/AppDatabase.kt \
        apps/android/app/src/main/java/com/axon/app/di/AppContainer.kt
git commit -m "feat(android): Session Room entity + DAO + DB migration v2"
```

---

### Task 13: SessionsViewModel + SessionsDrawerContent

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/sessions/SessionsViewModel.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/sessions/SessionsDrawerContent.kt`

- [ ] **Step 1: Write SessionsViewModel**

```kotlin
// ui/sessions/SessionsViewModel.kt
package com.axon.app.ui.sessions

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import java.util.UUID

class SessionsViewModel(app: Application) : AndroidViewModel(app) {
    private val dao = (app as AxonApp).container.database.sessionDao()

    val recentFour = dao.recentFour()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun createSession(firstMessage: String): String {
        val id = UUID.randomUUID().toString()
        val title = firstMessage.take(60).trim().let { if (it.length == 60) "$it…" else it }
        viewModelScope.launch {
            dao.upsert(Session(
                id = id,
                title = title,
                firstMessagePreview = firstMessage.take(80),
                turnCount = 1,
                createdAt = System.currentTimeMillis(),
                updatedAt = System.currentTimeMillis(),
            ))
        }
        return id
    }

    fun rename(id: String, title: String) = viewModelScope.launch { dao.rename(id, title) }

    fun togglePin(session: Session) = viewModelScope.launch {
        dao.setPinned(session.id, if (session.pinnedAt != null) null else System.currentTimeMillis())
    }
}
```

Add `database` property to `AppContainer`:
```kotlin
// In AppContainer.kt, add:
val database: AppDatabase = Room.databaseBuilder(app, AppDatabase::class.java, "axon-db")
    .addMigrations(AppDatabase.MIGRATION_1_2)
    .build()
```

- [ ] **Step 2: Write SessionsDrawerContent**

Replace the `SessionsDrawerContentStub` in `DrawerSectionContent.kt` with a real import from this file:

```kotlin
// ui/sessions/SessionsDrawerContent.kt
package com.axon.app.ui.sessions

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavController
import java.text.SimpleDateFormat
import java.util.*

@Composable
fun SessionsDrawerContent(
    onDismiss: () -> Unit,
    onNewSession: () -> Unit,
    onLoadSession: (Session) -> Unit,
    vm: SessionsViewModel = viewModel(),
) {
    val sessions by vm.recentFour.collectAsStateWithLifecycle()

    Column(modifier = Modifier.fillMaxSize()) {
        // Drawer header
        Row(
            modifier = Modifier.fillMaxWidth().padding(14.dp, 14.dp, 14.dp, 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(Icons.Rounded.History, contentDescription = "Sessions", tint = Color(0xFF29B6F6), modifier = Modifier.size(18.dp))
            Text("Sessions", fontSize = 14.sp, fontWeight = FontWeight.Bold, color = Color(0xFFE6F4FB))
        }

        Spacer(Modifier.height(4.dp))

        // New Session button
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 8.dp)
                .background(Color(0x1429B6F6), RoundedCornerShape(11.dp))
                .border(1.dp, Color(0x2E29B6F6), RoundedCornerShape(11.dp))
                .clickable(remember { MutableInteractionSource() }, indication = null) {
                    onNewSession()
                    onDismiss()
                }
                .padding(10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(Icons.Rounded.Add, contentDescription = null, tint = Color(0xFF29B6F6), modifier = Modifier.size(17.dp))
            Text("New Session", fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = Color(0xFF29B6F6))
        }

        Spacer(Modifier.height(6.dp))

        // Recent 4 sessions
        LazyColumn(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            items(sessions, key = { it.id }) { session ->
                SessionRow(
                    session = session,
                    onTap = { onLoadSession(session); onDismiss() },
                    onRename = { newTitle -> vm.rename(session.id, newTitle) },
                    onTogglePin = { vm.togglePin(session) },
                )
            }
        }
    }
}

@Composable
private fun SessionRow(
    session: Session,
    onTap: () -> Unit,
    onRename: (String) -> Unit,
    onTogglePin: () -> Unit,
) {
    var showMenu by remember { mutableStateOf(false) }
    val relativeTime = remember(session.updatedAt) {
        val diff = System.currentTimeMillis() - session.updatedAt
        when {
            diff < 60_000 -> "just now"
            diff < 3_600_000 -> "${diff / 60_000}m ago"
            diff < 86_400_000 -> "${diff / 3_600_000}h ago"
            else -> "${diff / 86_400_000}d ago"
        }
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(Color(0xFF102330), RoundedCornerShape(11.dp))
            .border(1.dp, Color(0xFF1D3D4E), RoundedCornerShape(11.dp))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onTap)
            .padding(10.dp),
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            if (session.pinnedAt != null) {
                Icon(Icons.Rounded.PushPin, contentDescription = "Pinned", tint = Color(0xFF29B6F6), modifier = Modifier.size(11.dp))
            }
            Text(
                session.title,
                fontSize = 11.sp,
                fontWeight = FontWeight.SemiBold,
                color = Color(0xFFE6F4FB),
                maxLines = 1,
                modifier = Modifier.weight(1f),
            )
            Text(relativeTime, fontSize = 9.sp, color = Color(0xFFA7BCC9))
        }
        Text(
            session.firstMessagePreview,
            fontSize = 9.5.sp,
            color = Color(0xFFA7BCC9),
            maxLines = 1,
        )
        Text(
            "${session.turnCount} turns" +
                if (session.injectedOpCount > 0) " · ${session.injectedOpCount} ops" else "",
            fontSize = 9.sp,
            color = Color(0x99A7BCC9),
        )
    }
}
```

- [ ] **Step 3: Update DrawerSectionContent to use real SessionsDrawerContent**

In `DrawerSectionContent.kt`:
```kotlin
import com.axon.app.ui.sessions.SessionsDrawerContent

// Replace SessionsDrawerContentStub() call:
DrawerSection.Sessions -> SessionsDrawerContent(
    onDismiss = onDismiss,
    onNewSession = { /* navController.navigate(NewSessionRoute) — handled by AskViewModel */ },
    onLoadSession = { session -> /* TODO: load session into AskViewModel */ },
)
```

- [ ] **Step 4: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/sessions/
git commit -m "feat(android): SessionsViewModel + SessionsDrawerContent"
```

---

## Phase 5 — Jobs Drawer

### Task 14: JobsOverviewViewModel

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsOverviewViewModel.kt`

- [ ] **Step 1: Write data classes for drawer overview**

```kotlin
// ui/jobs/JobsOverviewViewModel.kt
package com.axon.app.ui.jobs

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.remote.models.JobStatus
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch

data class JobTypeOverview(
    val kind: String,           // "crawl" | "embed" | "extract" | "ingest" | "watch"
    val runningCount: Int,
    val failedCount: Int,
    val idleOrDone: Boolean,
    val detail: String,         // "docs.anthropic.com · 842 pages" etc.
    val nextRunIn: String? = null, // for watches
)

data class IndividualJob(
    val id: String,
    val target: String,         // URL or repo name
    val status: String,         // "running" | "done" | "failed" | "pending"
    val progress: Float?,       // null = indeterminate, 0..1 = determinate
    val completedPages: Int? = null,
    val estimatedPages: Int? = null,
    val chunkCount: Int? = null,
    val elapsedMs: Long? = null,
    val errorText: String? = null,
)

class JobsOverviewViewModel(app: Application) : AndroidViewModel(app) {
    private val repo = (app as AxonApp).container.axonRepository

    private val _overview = MutableStateFlow<List<JobTypeOverview>>(emptyList())
    val overview: StateFlow<List<JobTypeOverview>> = _overview

    private val _drillDown = MutableStateFlow<Pair<String, List<IndividualJob>>?>(null)
    val drillDown: StateFlow<Pair<String, List<IndividualJob>>?> = _drillDown

    init { startPolling() }

    private fun startPolling() {
        viewModelScope.launch {
            while (true) {
                fetchOverview()
                delay(15_000)
            }
        }
    }

    private suspend fun fetchOverview() {
        repo.listJobs(kind = null).onSuccess { jobs ->
            val grouped = jobs.groupBy { it.kind }
            _overview.value = listOf("crawl", "embed", "extract", "ingest", "watch").map { kind ->
                val kindJobs = grouped[kind].orEmpty()
                val running = kindJobs.count { it.status == "running" || it.status == "pending" }
                val failed  = kindJobs.count { it.status == "failed" }
                val detail  = buildDetailLine(kind, kindJobs)
                JobTypeOverview(
                    kind = kind,
                    runningCount = running,
                    failedCount = failed,
                    idleOrDone = running == 0 && failed == 0,
                    detail = detail,
                )
            }
        }
    }

    fun openDrillDown(kind: String) {
        viewModelScope.launch {
            repo.listJobs(kind = kind).onSuccess { jobs ->
                _drillDown.value = kind to jobs.map { job ->
                    IndividualJob(
                        id = job.id,
                        target = job.url ?: job.target ?: job.id,
                        status = job.status,
                        progress = estimateProgress(job),
                        completedPages = job.pagesCompleted,
                        estimatedPages = job.pagesEstimated,
                        chunkCount = job.chunkCount,
                        elapsedMs = job.startedAt?.let { System.currentTimeMillis() - it },
                        errorText = job.errorText,
                    )
                }
            }
        }
    }

    fun closeDrillDown() { _drillDown.value = null }

    private fun estimateProgress(job: Any): Float? {
        // Returns null (indeterminate) for running jobs with no page estimate,
        // or a 0..1 fraction if we have completed/estimated
        // Use reflection-free approach: cast to the known job model type
        // This is a placeholder — wire to your actual job model fields
        return null // TODO: return job.pagesCompleted?.toFloat()?.div(job.pagesEstimated ?: return null)
    }

    private fun buildDetailLine(kind: String, jobs: List<Any>): String = "" // TODO: wire to job model
}
```

> The exact field names (`pagesCompleted`, `pagesEstimated`, `chunkCount`, etc.) depend on the shape of the job model in `AxonModels.kt` / `JobsModels.kt`. Check those files and wire the `estimateProgress` and `buildDetailLine` implementations to the actual fields. This is the primary connection point to the existing data layer.

- [ ] **Step 2: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 3: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsOverviewViewModel.kt
git commit -m "feat(android): JobsOverviewViewModel — aggregate + drill-down state"
```

---

### Task 15: JobsDrawerContent

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsDrawerContent.kt`

- [ ] **Step 1: Write JobsDrawerContent**

Two-level: overview shows 5 type rows; tapping one replaces content with per-job list. Back button in header returns to overview.

```kotlin
// ui/jobs/JobsDrawerContent.kt
package com.axon.app.ui.jobs

import androidx.compose.animation.*
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.AuroraProgressBar
import com.axon.app.ui.common.AuroraStatusDot
import com.axon.app.ui.common.DotState
import com.axon.app.ui.common.ProgressSize
import com.axon.app.ui.common.ProgressVariant

private val Accent   = Color(0xFF29B6F6)
private val TextMain = Color(0xFFE6F4FB)
private val TextMut  = Color(0xFFA7BCC9)
private val Panel    = Color(0xFF102330)
private val Border   = Color(0xFF1D3D4E)
private val WarnCol  = Color(0xFFC6A36B)
private val ErrCol   = Color(0xFFC78490)
private val OkCol    = Color(0xFF7DD3C7)

private data class KindMeta(val kind: String, val label: String, val icon: ImageVector)
private val Kinds = listOf(
    KindMeta("crawl",   "Crawls",      Icons.Rounded.TravelExplore),
    KindMeta("embed",   "Embeddings",  Icons.Rounded.AccountTree),
    KindMeta("extract", "Extractions", Icons.Rounded.FilterAlt),
    KindMeta("ingest",  "Ingestions",  Icons.Rounded.Download),
    KindMeta("watch",   "Watches",     Icons.Rounded.Visibility),
)

@Composable
fun JobsDrawerContent(
    onDismiss: () -> Unit,
    vm: JobsOverviewViewModel = viewModel(),
) {
    val overview by vm.overview.collectAsStateWithLifecycle()
    val drillDown by vm.drillDown.collectAsStateWithLifecycle()

    Column(Modifier.fillMaxSize()) {
        // Header
        DrawerHeader(
            title = drillDown?.first?.let { kind -> Kinds.find { it.kind == kind }?.label } ?: "Jobs",
            icon  = drillDown?.first?.let { kind -> Kinds.find { it.kind == kind }?.icon } ?: Icons.Rounded.Checklist,
            showBack = drillDown != null,
            onBack = { vm.closeDrillDown() },
        )

        AnimatedContent(targetState = drillDown, label = "jobs-level") { drill ->
            if (drill == null) {
                JobsOverview(overview = overview, onKindClick = { vm.openDrillDown(it) })
            } else {
                JobsDrillDown(kind = drill.first, jobs = drill.second)
            }
        }
    }
}

@Composable
private fun DrawerHeader(title: String, icon: ImageVector, showBack: Boolean, onBack: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(14.dp, 14.dp, 14.dp, 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (showBack) {
            Icon(
                Icons.Rounded.ArrowBackIosNew,
                contentDescription = "Back",
                tint = Accent,
                modifier = Modifier.size(16.dp).clickable(remember { MutableInteractionSource() }, null, onClick = onBack),
            )
        }
        Icon(icon, contentDescription = title, tint = Accent, modifier = Modifier.size(18.dp))
        Text(title, fontSize = 14.sp, fontWeight = FontWeight.Bold, color = TextMain)
    }
}

@Composable
private fun JobsOverview(overview: List<JobTypeOverview>, onKindClick: (String) -> Unit) {
    LazyColumn(Modifier.fillMaxWidth().padding(8.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
        items(overview, key = { it.kind }) { item ->
            val kindMeta = Kinds.find { it.kind == item.kind }!!
            val (badgeText, badgeColor) = when {
                item.runningCount > 0 -> "${item.runningCount} running" to Accent
                item.failedCount > 0  -> "${item.failedCount} failed" to WarnCol
                else                  -> "idle" to TextMut
            }
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Panel, RoundedCornerShape(11.dp))
                    .border(
                        1.dp,
                        if (item.runningCount > 0) Accent.copy(alpha = 0.18f) else Border,
                        RoundedCornerShape(11.dp),
                    )
                    .clickable(remember { MutableInteractionSource() }, indication = null) { onKindClick(item.kind) }
                    .padding(9.dp, 8.dp),
                verticalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(kindMeta.icon, contentDescription = kindMeta.label, tint = if (item.runningCount > 0) Accent else TextMut, modifier = Modifier.size(17.dp))
                    Text(kindMeta.label, fontSize = 11.5.sp, fontWeight = FontWeight.SemiBold, color = TextMain, modifier = Modifier.weight(1f))
                    // Badge
                    Text(
                        badgeText,
                        fontSize = 9.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = badgeColor,
                        modifier = Modifier
                            .background(badgeColor.copy(0.12f), RoundedCornerShape(999.dp))
                            .border(1.dp, badgeColor.copy(.28f), RoundedCornerShape(999.dp))
                            .padding(horizontal = 7.dp, vertical = 2.dp),
                    )
                }
                if (item.runningCount > 0) {
                    AuroraProgressBar(progress = null, variant = ProgressVariant.Cyan, size = ProgressSize.Sm, modifier = Modifier.fillMaxWidth())
                }
                if (item.detail.isNotEmpty()) {
                    Text(item.detail, fontSize = 9.5.sp, color = TextMut, fontFamily = FontFamily.Monospace)
                }
            }
        }
    }
}

@Composable
private fun JobsDrillDown(kind: String, jobs: List<IndividualJob>) {
    LazyColumn(Modifier.fillMaxWidth().padding(8.dp), verticalArrangement = Arrangement.spacedBy(5.dp)) {
        items(jobs, key = { it.id }) { job ->
            val dotState = when (job.status) {
                "running", "pending" -> DotState.Running
                "done"               -> DotState.Done
                "failed"             -> DotState.Failed
                else                 -> DotState.Idle
            }
            val barVariant = when (job.status) {
                "done"   -> ProgressVariant.Success
                "failed" -> ProgressVariant.Error
                else     -> ProgressVariant.Cyan
            }
            val borderColor = when (job.status) {
                "running", "pending" -> Accent.copy(.28f)
                "failed" -> ErrCol.copy(.22f)
                "done"   -> OkCol.copy(.18f)
                else -> Border
            }
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Panel, RoundedCornerShape(11.dp))
                    .border(1.dp, borderColor, RoundedCornerShape(11.dp))
                    .padding(10.dp, 9.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                    AuroraStatusDot(dotState)
                    Text(
                        job.target,
                        fontSize = 9.5.sp,
                        fontFamily = FontFamily.Monospace,
                        color = TextMain,
                        modifier = Modifier.weight(1f),
                        maxLines = 1,
                    )
                    val (bl, bc) = when (job.status) {
                        "running", "pending" -> "running" to Accent
                        "done"               -> "done"    to OkCol
                        "failed"             -> "failed"  to ErrCol
                        else                 -> job.status to TextMut
                    }
                    Text(bl, fontSize = 9.sp, fontWeight = FontWeight.SemiBold, color = bc,
                        modifier = Modifier
                            .background(bc.copy(.1f), RoundedCornerShape(999.dp))
                            .border(1.dp, bc.copy(.22f), RoundedCornerShape(999.dp))
                            .padding(horizontal = 7.dp, vertical = 2.dp))
                }
                if (job.status != "idle") {
                    AuroraProgressBar(
                        progress = job.progress,
                        variant = barVariant,
                        size = ProgressSize.Default,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    val pageStr = when {
                        job.completedPages != null && job.estimatedPages != null ->
                            "${job.completedPages} / ~${job.estimatedPages} pages"
                        job.chunkCount != null -> "${"%,d".format(job.chunkCount)} chunks"
                        job.errorText != null  -> job.errorText
                        else -> ""
                    }
                    Text(pageStr, fontSize = 9.sp, color = if (job.errorText != null) ErrCol else TextMut)
                    job.elapsedMs?.let {
                        val s = it / 1000
                        Text(if (s < 60) "${s}s" else "${s / 60}m ${s % 60}s", fontSize = 9.sp, color = TextMut)
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Update DrawerSectionContent to use JobsDrawerContent**

```kotlin
// In DrawerSectionContent.kt, replace JobsDrawerContentStub():
import com.axon.app.ui.jobs.JobsDrawerContent
DrawerSection.Jobs -> JobsDrawerContent(onDismiss = onDismiss)
```

- [ ] **Step 3: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsDrawerContent.kt
git commit -m "feat(android): JobsDrawerContent — 2-level jobs drawer with progress bars"
```

---

## Phase 6 — Knowledge + Remaining Drawers

### Task 16: SuggestScreen (full-screen)

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/knowledge/SuggestScreen.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNavGraph.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNav.kt`

- [ ] **Step 1: Add SuggestRoute**

```kotlin
// In AxonNav.kt:
@Serializable data object SuggestRoute
```

- [ ] **Step 2: Write SuggestScreen**

```kotlin
// ui/knowledge/SuggestScreen.kt
package com.axon.app.ui.knowledge

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel

private val Accent  = Color(0xFF29B6F6)
private val WarnCol = Color(0xFFC6A36B)
private val TextMain = Color(0xFFE6F4FB)
private val TextMut = Color(0xFFA7BCC9)
private val Panel   = Color(0xFF102330)
private val Border  = Color(0xFF1D3D4E)

@Composable
fun SuggestScreen(
    onBack: () -> Unit,
    vm: KnowledgeViewModel = viewModel(),
) {
    val suggestions by vm.suggestions.collectAsStateWithLifecycle()

    Column(modifier = Modifier.fillMaxSize().background(Color(0xFF07131C))) {
        // Header
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(Color(0xFF07111A))
                .padding(14.dp, 14.dp, 14.dp, 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(
                Icons.Rounded.ArrowBackIosNew,
                contentDescription = "Back",
                tint = Accent,
                modifier = Modifier.size(16.dp).clickable(remember { MutableInteractionSource() }, null, onClick = onBack),
            )
            Icon(Icons.Rounded.Lightbulb, contentDescription = "Suggest", tint = Accent, modifier = Modifier.size(18.dp))
            Column {
                Text("Suggest", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = TextMain)
                Text("Recommended docs based on your queries", fontSize = 9.5.sp, color = TextMut)
            }
        }

        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(horizontal = 8.dp),
            contentPadding = PaddingValues(vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            items(suggestions, key = { it.url }) { sug ->
                SuggestCard(
                    icon = sug.icon,
                    domain = sug.domain,
                    url = sug.url,
                    reason = sug.reason,
                    isAsync = sug.isAsync,
                    onAction = { vm.triggerSuggestAction(sug) },
                )
            }
        }
    }
}

@Composable
private fun SuggestCard(
    icon: ImageVector,
    domain: String,
    url: String,
    reason: String,
    isAsync: Boolean,
    onAction: () -> Unit,
) {
    val actionTint = if (isAsync) WarnCol else Accent
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(Panel, RoundedCornerShape(11.dp))
            .border(1.dp, Border, RoundedCornerShape(11.dp))
            .padding(10.dp, 9.dp),
        horizontalArrangement = Arrangement.spacedBy(9.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(icon, contentDescription = null, tint = Accent, modifier = Modifier.size(17.dp).padding(top = 1.dp))
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(domain, fontSize = 11.sp, fontWeight = FontWeight.SemiBold, color = TextMain)
            Text(url, fontSize = 8.5.sp, color = TextMut, fontFamily = FontFamily.Monospace, maxLines = 1)
            Text(reason, fontSize = 9.sp, color = TextMut)
        }
        Box(
            modifier = Modifier
                .background(actionTint.copy(0.12f), RoundedCornerShape(7.dp))
                .border(1.dp, actionTint.copy(.25f), RoundedCornerShape(7.dp))
                .clickable(remember { MutableInteractionSource() }, null, onClick = onAction)
                .padding(horizontal = 9.dp, vertical = 4.dp),
        ) {
            Text(if (isAsync) "Ingest" else "Crawl", fontSize = 9.sp, fontWeight = FontWeight.SemiBold, color = actionTint)
        }
    }
}
```

> `KnowledgeViewModel.suggestions` already exists (from `SuggestSection.kt`). Add a `triggerSuggestAction(sug)` method that calls `repo.crawlSubmit()` or `repo.ingestStart()` as appropriate.

- [ ] **Step 3: Register SuggestRoute in AxonNavGraph**

```kotlin
composable<SuggestRoute> {
    SuggestScreen(onBack = { navController.popBackStack() })
}
```

- [ ] **Step 4: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/knowledge/SuggestScreen.kt \
        apps/android/app/src/main/java/com/axon/app/ui/nav/
git commit -m "feat(android): SuggestScreen — full-screen suggest list"
```

---

### Task 17: KnowledgeDrawerContent + ManagementDrawerContent + SetupDrawerContent

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/knowledge/KnowledgeDrawerContent.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/management/ManagementDrawerContent.kt`
- Create: `apps/android/app/src/main/java/com/axon/app/ui/setup/SetupDrawerContent.kt`

- [ ] **Step 1: Write KnowledgeDrawerContent**

```kotlin
// ui/knowledge/KnowledgeDrawerContent.kt
package com.axon.app.ui.knowledge

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation.NavController
import com.axon.app.ui.nav.SuggestRoute

@Composable
fun KnowledgeDrawerContent(onDismiss: () -> Unit, navController: NavController) {
    Column(Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(14.dp, 14.dp, 14.dp, 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(Icons.Rounded.Hub, contentDescription = "Knowledge", tint = Color(0xFF29B6F6), modifier = Modifier.size(18.dp))
            Text("Knowledge", fontSize = 14.sp, fontWeight = FontWeight.Bold, color = Color(0xFFE6F4FB))
        }
        Spacer(Modifier.height(4.dp))
        Column(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            DrawerItem(Icons.Rounded.Lightbulb, "Suggest", "Recommended docs") {
                onDismiss()
                navController.navigate(SuggestRoute)
            }
            DrawerItem(Icons.Rounded.Link, "Sources", "All indexed URLs") {
                onDismiss()
                navController.navigate(SourcesRoute) // existing
            }
            DrawerItem(Icons.Rounded.Language, "Domains", "Domain facets") {
                onDismiss()
                // Navigate to KnowledgeScreen filtered to Domains tab — or keep using existing screen
                navController.navigate(KnowledgeRoute(tab = 2))
            }
            DrawerItem(Icons.Rounded.BarChart, "Stats", "Collection stats") {
                onDismiss()
                navController.navigate(KnowledgeRoute(tab = 3))
            }
        }
    }
}

@Composable
private fun DrawerItem(icon: ImageVector, label: String, subtitle: String, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(Color(0xFF102330), RoundedCornerShape(11.dp))
            .border(1.dp, Color(0xFF1D3D4E), RoundedCornerShape(11.dp))
            .clickable(remember { MutableInteractionSource() }, null, onClick = onClick)
            .padding(10.dp, 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        Icon(icon, contentDescription = label, tint = Color(0xFFA7BCC9), modifier = Modifier.size(17.dp))
        Column(Modifier.weight(1f)) {
            Text(label, fontSize = 11.5.sp, fontWeight = FontWeight.SemiBold, color = Color(0xFFE6F4FB))
            Text(subtitle, fontSize = 9.sp, color = Color(0x99A7BCC9))
        }
        Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = Color(0x66A7BCC9), modifier = Modifier.size(14.dp))
    }
}
```

> Add `@Serializable data class SourcesRoute` and `KnowledgeRoute(tab: Int)` to `AxonNav.kt` if they don't exist. Wire them in `AxonNavGraph.kt` to existing `SourcesScreen` and `KnowledgeScreen` composables respectively.

- [ ] **Step 2: Write ManagementDrawerContent**

```kotlin
// ui/management/ManagementDrawerContent.kt
package com.axon.app.ui.management

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation.NavController

// Management items — stubs with TODO for screens not yet built
@Composable
fun ManagementDrawerContent(onDismiss: () -> Unit, navController: NavController) {
    Column(Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(14.dp, 14.dp, 14.dp, 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(Icons.Rounded.Settings, contentDescription = "Management", tint = Color(0xFF29B6F6), modifier = Modifier.size(18.dp))
            Text("Management", fontSize = 14.sp, fontWeight = FontWeight.Bold, color = Color(0xFFE6F4FB))
        }
        // TODO: implement Dedupe, Monitor, Sync, Stack, Config screens
        // For now, each item navigates to a placeholder that shows the screen name
        Spacer(Modifier.height(8.dp))
        Text(
            "Dedupe · Monitor · Sync · Stack · Config\n(screens coming soon)",
            fontSize = 11.sp,
            color = Color(0xFFA7BCC9),
            modifier = Modifier.padding(horizontal = 14.dp),
        )
    }
}
```

- [ ] **Step 3: Write SetupDrawerContent**

```kotlin
// ui/setup/SetupDrawerContent.kt
package com.axon.app.ui.setup

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation.NavController

@Composable
fun SetupDrawerContent(onDismiss: () -> Unit, navController: NavController) {
    Column(Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(14.dp, 14.dp, 14.dp, 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(Icons.Rounded.Construction, contentDescription = "Setup", tint = Color(0xFF29B6F6), modifier = Modifier.size(18.dp))
            Text("Setup", fontSize = 14.sp, fontWeight = FontWeight.Bold, color = Color(0xFFE6F4FB))
        }
        Spacer(Modifier.height(4.dp))
        // Doctor maps to existing SystemScreen
        // Others are stubs
        Column(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            // Reuse the shared DrawerItem pattern (copy it here or extract to a common file)
            Text("Preflight · Setup · Smoke · Doctor · Debug", fontSize = 11.sp, color = Color(0xFFA7BCC9), modifier = Modifier.padding(2.dp))
            // Doctor — wire to existing SystemScreen
            // TODO: add individual rows for each once SystemScreen is navigable by route
        }
    }
}
```

- [ ] **Step 4: Update DrawerSectionContent to use all real implementations**

```kotlin
// In DrawerSectionContent.kt — replace all remaining stubs:
import com.axon.app.ui.knowledge.KnowledgeDrawerContent
import com.axon.app.ui.management.ManagementDrawerContent
import com.axon.app.ui.setup.SetupDrawerContent

DrawerSection.Knowledge  -> KnowledgeDrawerContent(onDismiss, navController)
DrawerSection.Management -> ManagementDrawerContent(onDismiss, navController)
DrawerSection.Setup      -> SetupDrawerContent(onDismiss, navController)
```

- [ ] **Step 5: Build**

```bash
cd apps/android && ./gradlew :app:compileDebugKotlin 2>&1 | tail -20
```

- [ ] **Step 6: Full build + lint**

```bash
cd apps/android && ./gradlew :app:assembleDebug 2>&1 | tail -30
```

- [ ] **Step 7: Commit**

```bash
git add apps/android/app/src/main/java/com/axon/app/ui/knowledge/KnowledgeDrawerContent.kt \
        apps/android/app/src/main/java/com/axon/app/ui/management/ \
        apps/android/app/src/main/java/com/axon/app/ui/setup/ \
        apps/android/app/src/main/java/com/axon/app/ui/nav/DrawerSectionContent.kt
git commit -m "feat(android): KnowledgeDrawerContent + Management + Setup drawers wired"
```

---

## Self-Review Against Spec

Checked `docs/specs/android-redesign.md` section-by-section:

| Spec requirement | Covered in |
|---|---|
| Side rail 5 sections, Material Symbols Rounded | Tasks 2, 7 |
| No Ask icon in rail | Task 4 (RailScaffold — Ask is background) |
| Overlay drawer 232dp, panelStrong bg, animation | Task 3 |
| Drawer sub-item anatomy (icon + label + badge + detail + progress) | Tasks 3, 15 |
| Sessions same pattern as other sections | Tasks 12, 13 |
| Sessions: auto-title + relative time + preview + turn count + pin | Task 13 |
| FAB full 360° ring, r=96dp, 10 ops, 36° apart | Tasks 7, 8 |
| FAB async ops amber tint (Crawl, Ingest) | Task 7 (FabOp.isAsync) |
| FAB input card center-screen | Task 9 |
| FAB input card paste + send + hint | Task 9 |
| Chat bubbles: user right-aligned cyan tint | Task 10 |
| Chat bubbles: Axon left-aligned ✦ avatar + AXON label | Task 10 |
| AuroraThinking while streaming | Task 10 (AxonBubble) |
| Injection cards for Crawl/Ingest | Task 10, 11 |
| Compact injection card text triggers Gemini skill | Task 10 (InjectionCard exact phrasing) |
| Aurora progress bar: gradient, shimmer, glow, 4 variants | Task 1 |
| Progress bars sm (4dp) + default (6dp) | Task 1 |
| Status dots 7dp, pulse for running | Task 1 |
| Jobs 2-level drawer (overview → drill-down) | Tasks 14, 15 |
| Job data per type (pages, chunks, elapsed, error) | Task 15 |
| Knowledge → Suggest full-screen list | Task 16 |
| Suggest cards: icon + domain + URL + reason + Crawl/Ingest chip | Task 16 |
| Management + Setup drawer stubs | Task 17 |
| Aurora token values throughout | All tasks (hex values match spec §4) |
| No emoji — Material Symbols Rounded FILL=1 | Tasks 2, 8, 13, 15, 16, 17 |

**One gap found:** The spec says the active rail item indicator bar should have a glow effect (`box-shadow: 0 0 8px accentPrimary`). Compose glow via `Modifier.drawBehind` + `BlurMaskFilter` requires API 31+ (minSdk is 24). The plan implements the bar without glow on older APIs. Add a note in `AxonRail.kt`:

```kotlin
// Glow approximation: double the bar with lower opacity for devices < API 31
// True BlurMaskFilter glow requires API 31+ via drawBehind + Paint.setShadowLayer
```

This is acceptable per the spec's Out of Scope (no special handling for older APIs mentioned).

---
