# Re-index Guide Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Write `docs/REINDEX-GUIDE.md` — a user-facing operations guide explaining what changed in schema v3, who needs to re-index, and how to do it safely.

**Architecture:** Single documentation file. No code changes. All facts sourced from `docs/contracts/qdrant-payload-schema.md`, `src/vector/ops/qdrant/utils.rs`, and existing CLI behaviour.

**Tech Stack:** Markdown. axon CLI commands only.

---

### Task 1: Write docs/REINDEX-GUIDE.md

**Files:**
- Create: `docs/REINDEX-GUIDE.md`

- [ ] **Step 1: Write the guide**

Full content is specified in the task description — write it verbatim.

- [ ] **Step 2: Verify the file exists and is well-formed**

```bash
wc -l docs/REINDEX-GUIDE.md
head -5 docs/REINDEX-GUIDE.md
```

Expected: file present, starts with `# Re-index Guide`.

- [ ] **Step 3: Commit**

```bash
git add docs/REINDEX-GUIDE.md docs/superpowers/plans/2026-05-21-reindex-guide.md
git commit -m "docs: add REINDEX-GUIDE for schema v3 payload upgrade"
```
