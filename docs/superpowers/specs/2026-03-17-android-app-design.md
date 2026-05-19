# Axon Android App — Design Spec

**Date**: 2026-03-17
**Status**: Draft
**Author**: jmagar + Claude

## Overview

Native Android app for interacting with the self-hosted Axon RAG system. Core experience is a Pulse-like ACP chat that streams through Axon's web API, with a conversation sidebar for history management.

## Scope

### In Scope (v1)
- RAG chat with full ACP event streaming (thinking, tool use, assistant deltas)
- Conversation sidebar with search, grouping, and new chat
- Local conversation persistence (Room/SQLite)
- Configurable server URL + API token (Tailscale network)
- File/image attachment support (composer)

### Out of Scope (v1)
- Dashboard / stats views
- Server-side conversation sync
- Push notifications
- Widget / quick actions
- Multi-server support

---

## Architecture

### Stack

| Layer | Technology |
|-------|-----------|
| **UI** | Jetpack Compose + Navigation Compose |
| **Architecture** | Single-Activity, MVVM with ViewModels |
| **Networking** | Retrofit/OkHttp (REST) + SSE client (chat streaming) |
| **Local Storage** | Room (conversations/messages) + EncryptedSharedPreferences (token) |
| **Preferences** | Jetpack DataStore (non-sensitive settings) |
| **DI** | Manual AppContainer (no Hilt/Dagger) |
| **Async** | Kotlin Coroutines + Flow |
| **Fonts** | Noto Sans 300-700 (body/headings) + Noto Sans Mono (code) |
| **Icons** | Lucide (matching web UI) |

### Why Native Kotlin (not Flutter/React Native)
- Direct access to Android APIs without bridge overhead
- Jetpack Compose is the modern Android standard
- Material Design 3 is first-class
- Single platform target (no iOS planned)

---

## API Surface

### SSE — Chat Streaming
- **Endpoint**: `POST /api/pulse/chat`
- **Auth**: `x-api-key` header
- **Request body**: `{ message, conversationId?, attachments? }`
- **Event types**:
  - `assistant_delta` — incremental text tokens
  - `thinking_content` — chain of thought text
  - `tool_use` — tool invocation (name, input)
  - `tool_use_update` — streaming tool output
  - `result` — final completion signal
  - `error` — error with message

### REST — Supporting Data
- `GET /api/cortex/stats` — collection stats (connection test)
- `GET /api/health` — server health check

### Auth
- Token: `AXON_WEB_BROWSER_API_TOKEN` (or `AXON_WEB_API_TOKEN` if no browser token)
- Injection: OkHttp interceptor adds `x-api-key` header to all `/api/*` requests
- Storage: Android `EncryptedSharedPreferences` (not plain DataStore)
- Validation: Hit `/api/cortex/stats` on save to test credentials

---

## Screens

### 1. Chat (Main Screen)

The primary experience. Full-screen conversation with streaming ACP events.

#### Header Bar
- **Left**: Orbit icon (sidebar trigger) — atomic orbital SVG, tappable to open drawer
- **Center**: Gradient "AXON" logo (blue→pink) + status dot
  - Green dot = connected
  - Pulsing pink dot = streaming/responding
- **Right**: Pink "+" button (new chat)

#### Message Bubbles
- **User** (right-aligned): Blue gradient bubble (`rgba(135,175,255,0.28→0.12)`), rounded `14px 14px 3px 14px`, avatar (nord-astro.png) to the right
- **Agent** (left-aligned): Pink gradient bubble (`rgba(255,135,175,0.1→0.55)`), rounded `3px 14px 14px 14px`, avatar (neon-astro.png) to the left

#### ACP Event Rendering
- **Thinking**: Collapsed by default — brain icon + duration (e.g. "2.3s"), chevron to expand. Auto-expands while actively streaming thinking content. Italic text with blue left border when expanded.
- **Tool Use**: Collapsed pill — chevron + search icon + tool name + query in cyan (#67e8f9). Expandable to show truncated results. Cyan border/text/icons throughout.
- **Sources**: Rendered below agent messages when present — file icon + path in primary blue, separated by subtle border.
- **Streaming cursor**: Thin blue blinking bar at end of in-progress text.

#### Composer
- **Left**: Attach button (paperclip icon, circular, subtle border)
- **Center**: Text input with "Ask anything..." placeholder
- **Right** (idle/has text): Pink send button (arrow-up icon)
- **Right** (streaming): Pink stop button (square icon, pulse glow animation) — replaces send. Attach button dims to 0.4 opacity. Input shows "Responding..." at 0.5 opacity.

### 2. Conversation Sidebar (Navigation Drawer)

Material 3 modal navigation drawer, opened by tapping the orbit icon.

#### Header
- Orbit icon + gradient "AXON" logo + green status dot + close X
- Search bar (compact) + pink "+" new chat button inline

#### Conversation List
- Grouped by time: Today, Yesterday, This Week, older date labels
- **Active conversation**: Glow background (`rgba(135,175,255,0.08)`)
- **Unread**: Blue dot indicator + bold title
- **Each item**: Title (truncated), timestamp, message count
- Font sizes: titles 11px, meta 9px, section headers 9px uppercase

#### Footer
- Settings gear + "Settings" label + version number

### 3. Settings Screen

Simple preferences form:
- Server URL (text input, validated on save)
- API Token (password input, stored in EncryptedSharedPreferences)
- Connection test button (hits `/api/cortex/stats`)
- Theme: System / Dark / Light (DataStore)
- About: version, build info

---

## Design Tokens

All values sourced from `docs/UI-DESIGN-SYSTEM.md`.

### Brand Colors
| Token | Value | Usage |
|-------|-------|-------|
| `axon-primary` | `#87afff` | Links, borders, orbit icon, status |
| `axon-secondary` | `#ff87af` | Send button, new chat, stop button, logo gradient end |
| `axon-success` | `#82d9a0` | Connected status dot |
| `axon-cyan` | `#67e8f9` | Tool use blocks (border, text, icons) |

### Text Ladder
| Token | Value | Usage |
|-------|-------|-------|
| `text-primary` | `#e8f4f8` | Message text, titles, bold inline |
| `text-secondary` | `#b8cfe0` | Agent message body text |
| `text-muted` | `#7a96b8` | Icons, settings labels |
| `text-dim` | `#4d6a8a` | Timestamps, meta, thinking text, placeholders |

### Surfaces
| Token | Value | Usage |
|-------|-------|-------|
| `axon-bg` | `#030817` | App background |
| `glass-panel` | `rgba(6,12,26,0.82)` | Header bar, sidebar, settings |
| `surface-input` | `rgba(10,18,35,0.32)` | Text input backgrounds |
| `border-subtle` | `rgba(135,175,255,0.15)` | Dividers, section borders |
| `border-standard` | `rgba(135,175,255,0.28)` | User bubble border |

### Typography
- **Body/Headings**: Noto Sans 300-700
- **Code**: Noto Sans Mono 400-600
- **Message text**: 12px, line-height 1.45
- **Meta/labels**: 9-10px
- **Sidebar titles**: 11px
- **Logo**: 13px, weight 700, letter-spacing 0.04em

### Background Gradient
App background includes subtle radial gradients:
- `radial-gradient(circle at 15% 35%, rgba(135,175,255,0.12), transparent 42%)`
- `radial-gradient(circle at 85% 20%, rgba(255,135,175,0.08), transparent 45%)`

---

## Data Layer

### Local Storage (Room)

```
conversations
├── id: String (UUID)
├── title: String
├── created_at: Long (epoch ms)
├── updated_at: Long (epoch ms)
├── message_count: Int
└── is_active: Boolean

messages
├── id: String (UUID)
├── conversation_id: String (FK → conversations)
├── role: String (user | assistant | thinking | tool_use | tool_result)
├── content: String
├── created_at: Long (epoch ms)
└── metadata_json: String? (sources, tool name, duration, etc.)
```

### Design Decisions
- **Local-only in v1**. No server-side conversation sync. The SSE chat endpoint is stateless — app sends a message, gets a streamed response, saves both sides to Room.
- **No sync, no conflict resolution**. Server is a pipe to the LLM + RAG context, not a conversation store.
- **Export, don't sync**. Future "share/export" button can dump conversations as `.jsonl` for `axon sessions` ingest into the knowledge base. One-way push.
- **v2 opportunity**: Cross-device conversation history would require a new sessions API on the server. Don't pre-build the plumbing.

### Preferences (DataStore)
- Server URL (String)
- Theme preference (enum: system/dark/light)
- Last active conversation ID

### Secrets (EncryptedSharedPreferences)
- API token

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| **No connection** | Banner: "Cannot reach Axon" with retry. Cached conversations readable. |
| **SSE disconnect mid-stream** | Show partial response + "Connection lost" indicator. Offer retry. |
| **Auth failure (401/403)** | Redirect to settings with "Invalid token" message. |
| **Timeout** | 30s for REST. No timeout on SSE (long-lived by design). |
| **Empty response** | Show "No response received" in agent bubble. |
| **Malformed SSE event** | Log + skip. Don't crash the stream for one bad event. |

### Retry Strategy
- REST: OkHttp retry interceptor, 3 attempts with exponential backoff (1s, 2s, 4s)
- SSE: Auto-reconnect on disconnect with backoff (2s, 4s, 8s, cap 30s). Reset backoff on successful connection that lasts >30s.

---

## Testing Strategy

| Layer | Scope | Tool |
|-------|-------|------|
| **Unit** | ViewModels, Room DAOs, SSE event parser, message formatting | JUnit 5 + Kotlin coroutines test |
| **Integration** | Retrofit services against mock server | MockWebServer (OkHttp) |
| **UI** | Compose components, critical flows | Compose UI tests + Preview |

### Key Test Cases
- SSE parser handles all 6 ACP event types correctly
- Conversation CRUD in Room (create, read, update title, delete)
- Auth interceptor injects header on `/api/*` but not other URLs
- Stop button cancels active SSE connection
- Sidebar search filters conversations by title
- Thinking block auto-expands during stream, collapses after

---

## Project Structure

```
app/
├── src/main/
│   ├── java/com/axon/app/
│   │   ├── AxonApp.kt                 # Application + AppContainer
│   │   ├── MainActivity.kt            # Single activity
│   │   ├── ui/
│   │   │   ├── theme/
│   │   │   │   ├── Theme.kt           # M3 theme with Axon tokens
│   │   │   │   ├── Color.kt           # Brand colors
│   │   │   │   └── Type.kt            # Noto Sans type scale
│   │   │   ├── chat/
│   │   │   │   ├── ChatScreen.kt       # Main chat composable
│   │   │   │   ├── ChatViewModel.kt    # Chat state + SSE management
│   │   │   │   ├── MessageBubble.kt    # User/agent bubble rendering
│   │   │   │   ├── ThinkingBlock.kt    # Collapsible thinking
│   │   │   │   ├── ToolUseBlock.kt     # Collapsible tool use (cyan)
│   │   │   │   ├── Composer.kt         # Input + attach + send/stop
│   │   │   │   └── StreamingCursor.kt  # Blinking cursor
│   │   │   ├── sidebar/
│   │   │   │   ├── SidebarDrawer.kt    # Navigation drawer
│   │   │   │   ├── ConversationList.kt # Grouped conversation items
│   │   │   │   └── ConversationItem.kt # Single conversation row
│   │   │   ├── settings/
│   │   │   │   ├── SettingsScreen.kt
│   │   │   │   └── SettingsViewModel.kt
│   │   │   └── components/
│   │   │       ├── OrbitIcon.kt        # Orbit SVG composable
│   │   │       ├── AxonLogo.kt         # Gradient text logo
│   │   │       ├── StatusDot.kt        # Green/pulsing pink dot
│   │   │       └── ErrorBanner.kt      # Connection error UI
│   │   ├── data/
│   │   │   ├── local/
│   │   │   │   ├── AppDatabase.kt      # Room database
│   │   │   │   ├── ConversationDao.kt
│   │   │   │   ├── MessageDao.kt
│   │   │   │   ├── Conversation.kt     # Entity
│   │   │   │   └── Message.kt          # Entity
│   │   │   ├── remote/
│   │   │   │   ├── AxonApiService.kt   # Retrofit interface
│   │   │   │   ├── SseClient.kt        # SSE connection + event parsing
│   │   │   │   ├── AuthInterceptor.kt  # x-api-key injection
│   │   │   │   └── AcpEvent.kt         # Sealed class for ACP events
│   │   │   └── repository/
│   │   │       ├── ChatRepository.kt   # Coordinates SSE + Room
│   │   │       └── SettingsRepository.kt
│   │   └── di/
│   │       └── AppContainer.kt         # Manual DI graph
│   └── res/
│       ├── drawable/                   # Avatars, icons
│       └── font/                       # Noto Sans family
├── build.gradle.kts
└── proguard-rules.pro
```

---

## Mockups

Interactive HTML mockups (all design decisions reflected):
- `screens-chat-v5.html` — Final: 3 phone frames (active chat, streaming, sidebar)

Served locally: `http://localhost:51269/screens-chat-v5.html`

---

## Design Decisions (Resolved)

1. **Attachment types**: Images and documents/text files. Use Android's file picker with MIME type filters for `image/*`, `text/*`, and `application/pdf`.
2. **Markdown rendering**: Full markdown — headers, lists, code blocks with syntax highlighting, inline formatting. Use a Compose markdown library (e.g. `compose-markdown` or `richtext-compose`).
3. **Conversation title generation**: LLM-generated. After the first assistant response, make a lightweight LLM call to generate a short title from the conversation context. Fallback: truncate first user message to 50 chars.
4. **Max conversation length**: Keep everything. No pruning — all messages persist in Room indefinitely. Room handles SQLite pagination efficiently for large conversations.
