# Lobe And Workflow Vision

Last Updated: 07:05:00 | 03/07/2026

## Purpose

This document captures the intended product model for Axon's future web UI.

The core idea is a deliberate split between:

- `Lobe`: the project home base
- `Workflow`: the live multi-session work surface

These are not the same thing and should not be forced into one layout.

## Core Product Split

### Lobe

A `Lobe` is a repo-scoped workspace identity for a project.

It is the project's:

- dashboard
- memory layer
- research hub
- planning surface
- control room
- launch point into the actual execution tools

The Lobe is not the primary 3-pane working interface.

### Workflow

The `Workflow` shell is the main operational work surface.

It is:

- global across projects
- session-centric rather than repo-centric
- optimized for active conversations, file handoff, and implementation flow

This is where the flexible pane choreography lives.

## Product Principles

### Omnibox First

The product should start from an omnibox and grow into a full workspace.

The user should be able to begin with:

- a repo path
- a crawl
- a few prompts

From that, Axon should be able to assemble:

- research context
- docs context
- session context
- roadmap
- PRD
- working surfaces

### Research And Planning First

The initial value comes from Axon's research and planning pipeline:

- crawler
- RAG
- QA
- PlateJS editor

That pipeline should feed directly into the Lobe and then into the Workflow shell.

### Sessions Matter

Sessions are not side data.

They are part of the core memory model for the project and for the active work surface.

Session resume and session review need to be first-class.

### Fluidity Over Density

The UI should feel fluid, not overstuffed.

The answer is not "show everything at once."

The answer is:

- better hierarchy
- stronger surface separation
- fluid pane transitions
- contextual reveal

### Visual Direction

Keep:

- the neural network background
- the blue / pink Axon color theme

Everything else can be redesigned.

The result should feel:

- clean
- modern
- beautiful
- fluid
- intentional

## Naming

### Lobe

The repo-scoped project workspace is called a `Lobe`.

This replaces the older vague notion of a generic session/workspace for project memory.

## Agent Model

Axon has ACP client support, which means the web UI can work with agent sessions from:

- Claude
- Codex
- Gemini
- Copilot

The system should surface these sessions in a unified way.

## Lobe Definition

A Lobe is repo-based.

Given a repo/folder, Axon should discover and organize all sessions associated with that repo.

The Lobe should unify:

- repo state
- agent sessions
- research data
- planning data
- operational data

## Lobe Data Model

Each Lobe should have its own Qdrant collection.

That collection should index:

- all sessions from all supported agents for that repo
- the repo itself
- logs
- PR reviews
- PR comments
- issues
- relevant docs surfaced from the main Cortex collection

### Cortex Relationship

The main `cortex` collection remains the broader knowledge base.

A Lobe should derive relevant docs from Cortex by:

1. ingesting repo and session content
2. identifying the tech stack
3. semantically finding relevant docs already crawled in Cortex
4. surfacing those docs as part of the project's local working context

Example:

- if the repo uses Next.js, Rust, and PlateJS
- the Lobe docs surface should show those doc families
- selecting a specific doc should open it in PlateJS

## Lobe Responsibilities

The Lobe is the project home base for as many of these surfaces as possible:

- repo identity
- branches
- PRs
- review comments
- review state
- issues
- README
- file explorer
- docs explorer
- all repo sessions from Claude, Codex, Gemini, Copilot
- ability to resume or review sessions
- CI/CD status
- project stats derived from agent sessions
- roadmap
- notes
- todos
- logs
- jobs
- terminal access
- lobe-scoped search
- Qdrant collection info and stats
- suggested docs to crawl
- relevant docs from Cortex
- MCP configuration
- skills
- AI config files

### AI Config Files

These should be easy to inspect and edit:

- `AGENTS.md`
- `CLAUDE.md`
- `GEMINI.md`
- `SKILL.md` files
- agent definitions
- commands
- `.mcp.json`

## Lobe UX Model

The Lobe should begin with a create/load flow.

### Create / Load

The user needs a clear flow to:

- create a new Lobe
- load an existing Lobe

This is mandatory.

### Lobe Ignition Flow

A strong mock flow for the Lobe should be:

1. start with the omnibox
2. seed the Lobe by mocking a crawl of docs
3. ingest/mock sessions and repo memory
4. show an agent conversation about the planned project direction
5. surface a PRD in the editor

This demonstrates how Axon grows from prompt to working project context.

### Lobe Should Not Be A 3-Pane Shell

The Lobe does not need to be the main 3-pane layout.

It is a project dashboard and project control surface, not the primary chat/editor cockpit.

## Lobe Information Architecture

The Lobe should emphasize project state, not raw density.

It should act as the stepping stone to:

- editor
- terminal
- docs
- file explorer
- logs
- sessions
- agents/chat
- jobs
- todos
- skills
- MCP

The Lobe should make those surfaces reachable and understandable without making them all equally expanded at once.

## Lobe Explorer Model

For the file explorer area, the user wants a switchable tree with tabs or a segmented control.

At minimum:

- `Repo files`
- `Docs`

This allows the same region to switch between repo structure and crawled/relevant docs.

## Docs Surface Expectations

The docs area should:

- show relevant doc families based on the repo's actual stack
- allow browsing by doc set, for example `Next.js`, `Rust`, `PlateJS`
- open a selected doc into PlateJS

## Workflow Definition

The Workflow shell is the actual work interface.

It is separate from the Lobe.

It should be global across projects and optimized for active execution.

## Workflow Layout

The Workflow shell should use a flexible 3-pane system.

### Left Pane

The left sidebar is a global session rail.

It should show recent/current sessions across all projects.

Each session row should show:

- repo
- branch
- agent
- auto-generated session name

This pane should be the smallest pane by default.

### Center Pane

The center pane is the conversation surface.

Selecting a session from the sidebar opens the conversation here.

### Right Pane

The right pane is the editor.

It opens contextually:

- clicking a file mentioned in chat
- clicking an artifact
- using a keybind

The editor should not have to stay open all the time.

### Bottom Drawer

The terminal should appear from the bottom as a drawer.

It should be quickly toggleable by:

- button
- keybind

## Workflow Pane Behavior

All panes should support:

- collapse
- expand
- resize
- fluid transitions

The user should be able to work in any of these states:

- sidebar only
- sidebar + chat
- sidebar + editor
- sidebar + chat + editor
- chat only
- editor only
- chat + editor
- sidebar + omnibox

The interface should feel joyful and fluid when panes open and close.

## Workflow Interaction Model

The intended interaction flow is:

1. start from omnibox + sidebar
2. select or begin a session
3. conversation opens
4. mentioned files or artifacts open the editor
5. terminal appears from the bottom when implementation starts

This lets the workspace grow in complexity only when needed.

## Session Handling

Session handling needs to improve significantly.

Current issues called out by the user:

- creating a new thread/session is awkward
- resuming a legitimate session is not well supported

Target behavior:

- sessions are easy to browse
- sessions are easy to resume
- sessions are easy to review later
- sessions are unified across supported agents

## Relationship Between Lobe And Workflow

### Lobe

Use when you need:

- project understanding
- project memory
- research
- planning
- docs
- roadmap
- project health
- repo context

### Workflow

Use when you need:

- active execution
- live session work
- chat
- file handoff
- implementation
- terminal

The Lobe feeds the Workflow.

The Workflow should be able to link back to the Lobe.

## Suggested Route Model

A clean prototype structure is:

- `/reboot`
  shell chooser / overview
- `/reboot/lobe`
  project-scoped Lobe shell
- `/reboot/workflow`
  global Workflow shell

## Mock Expectations

The mock should visualize as many of these ideas as possible without becoming unreadable.

This includes:

- create/load Lobe flow
- omnibox ignition
- crawl/doc seeding
- agent session handoff
- PRD reveal
- docs explorer tabs
- session rail
- contextual editor behavior
- terminal drawer

## Implementation Notes

### AI Elements

AI Elements is a good fit for the mock and should be used where useful, especially for:

- conversation
- message rendering
- reasoning
- queue / workflow state
- attachments / file context

### PlateJS

PlateJS remains the target editor surface for:

- PRDs
- notes
- roadmap
- docs viewing

### ACP Integration

Because Axon now supports ACP clients, the Lobe and Workflow model must assume:

- multiple agent ecosystems
- shared repo context
- session retrieval across agents

## Non-Goals

The system should not:

- collapse Lobe and Workflow into one overloaded screen
- attempt to show every data surface with equal prominence
- make the 3-pane shell the project dashboard

## Canonical Product Statement

Axon should let a user start with an omnibox, seed a repo-scoped Lobe through crawl and session memory, derive research and planning context, then step into a fluid global Workflow shell where sessions, chat, editor, and terminal work together cleanly.
