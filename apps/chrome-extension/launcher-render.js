/* ============================================================
 * Axon launcher — action-result renders
 * Plain-DOM ports of the design handoff renders (Reference/axon/
 * renders.jsx). Each render takes a normalized `data` object plus a
 * tone trio and returns a DOM node. Normalizers (PREP) map the real
 * /v1/* responses onto the shapes the renders expect; anything they
 * cannot map falls back to GenericResult (readable JSON).
 * ============================================================ */

(function () {
  const { iconSvg } = window.AxonIcons;
  const { tint, hostOf } = window.AxonData;

  /* ── tiny DOM builder (React-inline-style → element) ── */
  const UNITLESS = new Set([
    "opacity", "fontWeight", "zIndex", "order", "flex", "flexGrow", "flexShrink",
    "lineHeight", "fontVariationSettings",
  ]);
  function applyStyle(node, style) {
    for (const [k, v] of Object.entries(style)) {
      if (v == null) continue;
      node.style[k] = typeof v === "number" && !UNITLESS.has(k) ? `${v}px` : v;
    }
  }
  function append(node, children) {
    if (children == null || children === false) return;
    if (Array.isArray(children)) { children.forEach((c) => append(node, c)); return; }
    if (children instanceof Node) { node.appendChild(children); return; }
    node.appendChild(document.createTextNode(String(children)));
  }
  function el(tag, props, children) {
    const node = document.createElement(tag);
    if (props) {
      for (const [k, v] of Object.entries(props)) {
        if (v == null) continue;
        if (k === "style") applyStyle(node, v);
        else if (k === "class") node.className = v;
        else if (k === "html") node.innerHTML = v;
        else if (k.startsWith("on") && typeof v === "function") node.addEventListener(k.slice(2).toLowerCase(), v);
        else node.setAttribute(k, v);
      }
    }
    append(node, children);
    return node;
  }
  const Icon = (name, size, color, stroke) => iconSvg(name, { size: size || 16, color, stroke });

  /* ── shared primitives ── */
  function Bar(pct, tone, h = 5) {
    return el("div", { style: { height: h, borderRadius: 999, background: "var(--axon-control)", border: "1px solid var(--aurora-border-default)", overflow: "hidden", minWidth: 36 } }, [
      el("div", { style: { width: `${Math.max(2, Math.min(100, pct || 0))}%`, height: "100%", borderRadius: 999, background: `linear-gradient(90deg, ${tone.deep}, ${tone.base})` } }),
    ]);
  }
  function Card(children, style) {
    return el("div", { style: { borderRadius: 12, background: "var(--axon-control)", border: "1px solid var(--aurora-border-default)", padding: "12px 13px", ...(style || {}) } }, children);
  }
  function Mono(children, color, size = 11.5) {
    return el("span", { style: { fontFamily: "var(--font-mono)", fontSize: size, color: color || "var(--aurora-text-muted)" } }, children);
  }
  function col(items, gap) {
    return el("div", { style: { display: "flex", flexDirection: "column", gap } }, items);
  }

  /* ── markdown blocks (scrape / summarize / retrieve / default) ── */
  function MdBlocks(blocks, tones) {
    return el("div", { style: { display: "flex", flexDirection: "column", gap: 11 } }, (blocks || []).map((b) => {
      if (b.t === "h1") return el("h1", { style: { margin: 0, fontFamily: "var(--font-display)", fontWeight: 800, fontSize: 23, letterSpacing: "-0.03em", color: "var(--aurora-text-primary)", lineHeight: 1.12 } }, b.x);
      if (b.t === "h2") return el("h2", { style: { margin: "6px 0 0", display: "flex", alignItems: "center", gap: 9, fontFamily: "var(--font-display)", fontWeight: 800, fontSize: 16, color: "var(--aurora-text-primary)" } }, [
        el("span", { style: { width: 4, height: 15, borderRadius: 999, background: tones.base, flex: "0 0 4px" } }), b.x,
      ]);
      if (b.t === "p") return el("p", { style: { margin: 0, fontFamily: "var(--font-sans)", fontSize: 14, lineHeight: 1.6, color: "var(--aurora-text-primary)", overflowWrap: "anywhere" } }, b.x);
      if (b.t === "ul") return el("ul", { style: { margin: 0, padding: 0, listStyle: "none", display: "flex", flexDirection: "column", gap: 8 } }, b.x.map((li) => {
        const [c, ...r] = String(li).split(" — ");
        return el("li", { style: { display: "flex", gap: 10, alignItems: "baseline", fontFamily: "var(--font-sans)", fontSize: 14, lineHeight: 1.5, color: "var(--aurora-text-primary)" } }, [
          el("span", { style: { width: 5, height: 5, borderRadius: 999, background: tones.base, flex: "0 0 5px", transform: "translateY(7px)" } }),
          el("span", null, [
            el("code", { style: { fontFamily: "var(--font-mono)", fontSize: 12.5, color: tones.fg } }, c),
            r.length ? el("span", { style: { color: "var(--aurora-text-muted)" } }, ` — ${r.join(" — ")}`) : null,
          ]),
        ]);
      }));
      if (b.t === "code") return el("div", { style: { borderRadius: 11, overflow: "hidden", border: "1px solid var(--aurora-border-default)", background: "var(--axon-page)" } }, [
        el("div", { style: { display: "flex", alignItems: "center", gap: 7, padding: "7px 12px", borderBottom: "1px solid var(--aurora-border-default)", color: "var(--aurora-text-muted)" } }, [Icon("hash", 11), Mono(b.lang || "code", null, 10.5)]),
        el("pre", { style: { margin: 0, padding: "12px 14px", overflowX: "auto", fontFamily: "var(--font-mono)", fontSize: 12.5, lineHeight: 1.6, color: "var(--aurora-text-primary)" } }, b.x),
      ]);
      return null;
    }));
  }

  /* ── query → ranked semantic hits ── */
  function QueryHits(data, tones) {
    return col([
      Mono(`${data.results.length} hits · reranked`),
      ...data.results.map((r) => Card([
        el("div", { style: { display: "flex", alignItems: "center", gap: 9 } }, [
          el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 11, fontWeight: 700, color: tones.fg, width: 20, flex: "0 0 auto" } }, `#${r.rank}`),
          el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 12.5, color: "var(--aurora-accent-strong)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flex: 1, minWidth: 0 } }, hostOf(r.url)),
          Mono(`c${r.chunk_index}`, null, 10.5),
        ]),
        el("p", { style: { margin: 0, fontFamily: "var(--font-sans)", fontSize: 13, lineHeight: 1.5, color: "var(--aurora-text-primary)", overflowWrap: "anywhere" } }, r.snippet),
        el("div", { style: { display: "flex", alignItems: "center", gap: 10 } }, [
          Mono(`score ${Number(r.score).toFixed(3)}`, null, 10),
          el("div", { style: { flex: 1 } }, Bar((r.rerank_score || 0) * 100, tones, 4)),
          Mono(`rerank ${Number(r.rerank_score).toFixed(2)}`, tones.fg, 10),
        ]),
      ], { display: "flex", flexDirection: "column", gap: 8 })),
    ], 9);
  }

  /* ── search / research → web results ── */
  function WebResults(data, tones, kind) {
    const items = [];
    if (kind === "research" && data.summary) {
      items.push(Card([
        el("div", { style: { display: "inline-flex", alignItems: "center", gap: 7, marginBottom: 7 } }, [Icon("sparkles", 13, tones.fg), Mono("SYNTHESIS", tones.fg, 10.5)]),
        el("p", { style: { margin: 0, fontFamily: "var(--font-sans)", fontSize: 14, lineHeight: 1.6, color: "var(--aurora-text-primary)" } }, data.summary),
      ], { background: tint(tones.base, 8, "var(--axon-control)"), border: `1px solid ${tint(tones.base, 26)}` }));
    }
    if (kind === "search" && data.auto_crawl_status) {
      items.push(el("div", { style: { display: "flex", alignItems: "center", gap: 8, fontFamily: "var(--font-mono)", fontSize: 11, color: "var(--aurora-text-muted)" } }, [
        Icon("crawl", 13, "var(--axon-orange-strong)"), "auto-crawl: ",
        el("span", { style: { color: "var(--aurora-success)" } }, `${data.auto_crawl_status.enqueued} enqueued`),
        ` · ${data.auto_crawl_status.skipped} skipped`,
      ]));
    }
    (data.results || []).forEach((r) => {
      items.push(Card([
        el("div", { style: { display: "flex", alignItems: "baseline", gap: 8 } }, [
          el("span", { style: { fontFamily: "var(--font-display)", fontWeight: 700, fontSize: 14, color: "var(--aurora-text-primary)", flex: 1, minWidth: 0 } }, r.title),
          r.score != null ? Mono(`${(r.score * 100).toFixed(0)}`, tones.fg, 10) : null,
        ]),
        Mono(hostOf(r.url), "var(--aurora-accent-strong)", 11.5),
        r.snippet ? el("p", { style: { margin: 0, fontFamily: "var(--font-sans)", fontSize: 13, lineHeight: 1.5, color: "var(--aurora-text-muted)", overflowWrap: "anywhere" } }, r.snippet) : null,
      ], { display: "flex", flexDirection: "column", gap: 6 }));
    });
    return col(items, 10);
  }

  /* ── summarize → single synthesized summary ── */
  function SummaryCard(data, tones) {
    return col([
      el("div", { style: { display: "flex", gap: 8, flexWrap: "wrap" } }, [
        ...(data.urls || []).map((u) => el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 11, color: "var(--aurora-accent-strong)", padding: "3px 9px", borderRadius: 999, background: tint(tones.base, 10, "var(--axon-control)"), border: `1px solid ${tint(tones.base, 26)}` } }, hostOf(u))),
        data.context_chars != null ? Mono(`${Number(data.context_chars).toLocaleString()} ctx chars${data.context_truncated ? " · truncated" : ""}`) : null,
      ]),
      Card([el("p", { style: { margin: 0, fontFamily: "var(--font-sans)", fontSize: 14, lineHeight: 1.62, color: "var(--aurora-text-primary)" } }, data.summary)], { background: tint(tones.base, 8, "var(--axon-control)"), border: `1px solid ${tint(tones.base, 26)}` }),
    ], 10);
  }

  /* ── map → discovered URL list ── */
  function MapUrls(data, tones) {
    return col([
      Mono(`${data.mapped_urls} of ${data.total} URLs · bodies skipped`),
      Card(data.urls.map((u, i) => el("div", { style: { display: "flex", alignItems: "center", gap: 9, padding: "7px 8px", borderTop: i ? "1px solid var(--aurora-border-default)" : "none" } }, [
        Icon("link", 12, tones.fg),
        el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 12, color: "var(--aurora-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } }, hostOf(u)),
      ])), { padding: "6px 6px", display: "flex", flexDirection: "column" }),
    ], 8);
  }

  /* ── sources → the indexed-doc library ── */
  function SourcesList(data, tones, onOpenDoc) {
    const totalChunks = data.urls.reduce((n, u) => n + (u[1] || 0), 0);
    return col([
      Mono(`${data.count} documents · ${totalChunks.toLocaleString()} chunks`),
      ...data.urls.map(([url, chunks]) => el("button", {
        style: { all: "unset", cursor: onOpenDoc ? "pointer" : "default", boxSizing: "border-box", width: "100%" },
        ...(onOpenDoc ? { onclick: () => onOpenDoc(url) } : {}),
      }, Card([
        el("span", { style: { width: 34, height: 34, flex: "0 0 34px", borderRadius: 9, display: "grid", placeItems: "center", color: tones.fg, background: tint(tones.base, 12, "var(--axon-page)"), border: `1px solid ${tint(tones.base, 26)}` } }, Icon("file", 16)),
        el("div", { style: { flex: 1, minWidth: 0 } }, [
          el("div", { style: { fontFamily: "var(--font-sans)", fontWeight: 650, fontSize: 13.5, color: "var(--aurora-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } }, hostOf(url).split("/").slice(1).join("/") || hostOf(url)),
          el("div", { style: { marginTop: 2, display: "flex", gap: 8, alignItems: "center" } }, [Mono(hostOf(url).split("/")[0], "var(--aurora-accent-strong)", 11)]),
        ]),
        el("div", { style: { textAlign: "right", flex: "0 0 auto" } }, [
          el("div", { style: { fontFamily: "var(--font-mono)", fontSize: 13, fontWeight: 700, color: tones.fg, fontVariantNumeric: "tabular-nums" } }, Number(chunks || 0).toLocaleString()),
          Mono("chunks", null, 9.5),
        ]),
        onOpenDoc ? Icon("chevronRight", 16, "var(--aurora-text-muted)") : null,
      ], { display: "flex", alignItems: "center", gap: 11 }))),
    ], 8);
  }

  /* ── retrieve → the doc viewer body ── */
  function RetrieveDoc(data, tones) {
    return col([
      Card([
        el("div", { style: { display: "flex", alignItems: "center", gap: 8 } }, [
          Icon("link", 13, tones.fg),
          el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 12, color: "var(--aurora-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } }, hostOf(data.matched_url)),
        ]),
        el("div", { style: { display: "flex", gap: 14, flexWrap: "wrap" } }, [
          el("span", { style: { display: "inline-flex", alignItems: "center", gap: 6 } }, [Icon("layers", 12, tones.fg), Mono(`${Number(data.chunk_count || 0).toLocaleString()} chunks`, tones.fg)]),
          el("span", { style: { display: "inline-flex", alignItems: "center", gap: 6 } }, [Icon("hash", 12), Mono(`~${Number(data.token_estimate || 0).toLocaleString()} tokens`)]),
          el("span", { style: { display: "inline-flex", alignItems: "center", gap: 6 } }, [el("span", { style: { width: 6, height: 6, borderRadius: 999, background: "var(--aurora-success)", boxShadow: "0 0 6px var(--aurora-success)" } }), Mono(data.refresh_status || "fresh", "var(--aurora-success)")]),
        ]),
      ], { display: "flex", flexDirection: "column", gap: 9, background: "var(--axon-page)" }),
      MdBlocks(data.blocks, tones),
    ], 12);
  }

  /* ── stats → collection metrics ── */
  function StatsGrid(data, tones) {
    const p = data.payload;
    const cells = [
      { label: "Documents", value: num(p.documents), color: tones.fg },
      { label: "Chunks", value: num(p.chunks) },
      { label: "Vectors", value: num(p.vectors) },
      { label: "Domains", value: p.domains != null ? p.domains : "—" },
      { label: "Dim", value: p.dim != null ? p.dim : "—" },
      { label: "Storage", value: p.storage_bytes != null ? `${(p.storage_bytes / 1e6).toFixed(0)} MB` : "—" },
    ];
    const rows = [
      ["collection", p.collection], ["mode", p.mode], ["embedding model", p.embedding_model], ["last indexed", p.last_indexed],
    ].filter(([, v]) => v != null);
    return col([
      el("div", { style: { display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 9 } }, cells.map((c) => Card([
        el("div", { class: "aurora-muted-label", style: { fontSize: 9.5, letterSpacing: "0.14em" } }, c.label),
        el("div", { style: { marginTop: 5, fontFamily: "var(--font-display)", fontWeight: 800, fontSize: 20, letterSpacing: "-0.03em", fontVariantNumeric: "tabular-nums", color: c.color || "var(--aurora-text-primary)" } }, c.value),
      ], { padding: "11px 12px" }))),
      rows.length ? Card(rows.map(([k, v]) => el("div", { style: { display: "flex", justifyContent: "space-between" } }, [Mono(k), Mono(String(v), k === "last indexed" ? "var(--aurora-success)" : "var(--aurora-text-primary)")])), { display: "flex", flexDirection: "column", gap: 7 }) : null,
    ], 11);
  }

  /* ── domains → vector counts per domain ── */
  function DomainsList(data, tones) {
    const max = Math.max(1, ...data.domains.map((d) => d.vectors || 0));
    return col([
      Mono(`${data.domains.length} domains`),
      Card(data.domains.map((d) => el("div", { style: { display: "flex", alignItems: "center", gap: 11 } }, [
        el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 12.5, color: "var(--aurora-text-primary)", width: 150, flex: "0 0 auto", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } }, d.domain),
        el("div", { style: { flex: 1 } }, Bar(((d.vectors || 0) / max) * 100, tones, 6)),
        Mono(Number(d.vectors || 0).toLocaleString(), tones.fg, 11),
      ])), { display: "flex", flexDirection: "column", gap: 11 }),
    ], 8);
  }

  /* ── suggest → gap analysis with crawl/ingest chips ── */
  function SuggestList(data, tones, onAct) {
    return col([
      Mono(`${data.urls.length} suggestions · gaps in the corpus`),
      ...data.urls.map((s) => {
        const ingest = /github\.com|reddit\.com|youtube\.com/.test(s.url);
        const chipTone = ingest ? "var(--axon-orange)" : "var(--aurora-accent-primary)";
        const chipFg = ingest ? "var(--axon-orange-strong)" : "var(--aurora-accent-strong)";
        return Card([
          Icon(ingest ? "box" : "scrape", 16, tones.fg),
          el("div", { style: { flex: 1, minWidth: 0 } }, [
            Mono(hostOf(s.url), "var(--aurora-accent-strong)", 12),
            s.reason ? el("div", { style: { marginTop: 3, fontFamily: "var(--font-sans)", fontSize: 12, fontStyle: "italic", color: "var(--aurora-text-muted)" } }, s.reason) : null,
          ]),
          el("button", { style: { all: "unset", cursor: "pointer", fontFamily: "var(--font-sans)", fontSize: 12, fontWeight: 650, color: chipFg, padding: "5px 11px", borderRadius: 8, background: tint(chipTone, 12, "var(--axon-control)"), border: `1px solid ${tint(chipTone, 30)}`, flex: "0 0 auto" }, ...(onAct ? { onclick: () => onAct(ingest ? "ingest" : "crawl", s.url) } : {}) }, ingest ? "Ingest" : "Crawl"),
        ], { display: "flex", alignItems: "center", gap: 11 });
      }),
    ], 9);
  }

  /* ── doctor → service health ── */
  function DoctorReport(data, tones) {
    const p = data.payload;
    const dot = (s) => (s === "ok" ? "var(--aurora-success)" : s === "warn" ? "var(--aurora-warn)" : "var(--aurora-error)");
    return col([
      el("div", { style: { display: "inline-flex", alignItems: "center", gap: 8 } }, [
        Icon("check", 14, p.ok ? "var(--aurora-success)" : "var(--aurora-error)"),
        el("span", { style: { fontFamily: "var(--font-display)", fontWeight: 800, fontSize: 15, color: "var(--aurora-text-primary)" } }, p.ok ? "Stack healthy" : "Issues found"),
      ]),
      ...p.services.map((s) => Card([
        el("span", { style: { width: 8, height: 8, borderRadius: 999, flex: "0 0 auto", background: dot(s.status), boxShadow: `0 0 6px ${dot(s.status)}` } }),
        el("div", { style: { flex: 1, minWidth: 0 } }, [
          el("div", { style: { fontFamily: "var(--font-sans)", fontWeight: 650, fontSize: 13.5, color: "var(--aurora-text-primary)" } }, s.name),
          s.detail ? Mono(s.detail, null, 11) : null,
        ]),
        Mono(s.ms == null ? "skipped" : `${s.ms} ms`, s.ms == null ? "var(--aurora-warn)" : "var(--aurora-text-muted)", 11),
      ], { display: "flex", alignItems: "center", gap: 11 })),
    ], 9);
  }

  /* ── status → running/pending job rollup ── */
  function StatusReport(data, tones) {
    const p = data.payload;
    return col([
      el("div", { style: { display: "grid", gridTemplateColumns: "1fr 1fr", gap: 9 } }, [
        Card([el("div", { class: "aurora-muted-label", style: { fontSize: 9.5, letterSpacing: "0.14em" } }, "Running"), el("div", { style: { marginTop: 5, fontFamily: "var(--font-display)", fontWeight: 800, fontSize: 22, color: tones.fg } }, p.running)], { padding: "11px 12px" }),
        Card([el("div", { class: "aurora-muted-label", style: { fontSize: 9.5, letterSpacing: "0.14em" } }, "Pending"), el("div", { style: { marginTop: 5, fontFamily: "var(--font-display)", fontWeight: 800, fontSize: 22, color: "var(--aurora-text-primary)" } }, p.pending)], { padding: "11px 12px" }),
      ]),
      Object.keys(p.by_type || {}).length ? Card(Object.entries(p.by_type).map(([k, v]) => el("div", { style: { display: "flex", alignItems: "center", gap: 10 } }, [
        el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 12.5, color: "var(--aurora-text-primary)", flex: 1, textTransform: "capitalize" } }, k),
        Mono(`${v.running} running`, v.running ? tones.fg : "var(--aurora-text-muted)"),
        Mono(`· ${v.pending} pending`),
      ])), { display: "flex", flexDirection: "column", gap: 8 }) : null,
    ], 10);
  }

  /* ── ingest / embed / extract / crawl → accepted async job ── */
  function JobAccepted(data, op, tones) {
    const rows = [];
    if (data.source_type) rows.push(["source", data.source_type]);
    if (data.target) rows.push(["target", data.target]);
    rows.push(["job id", data.job_id || "—"]);
    return Card([
      el("div", { style: { display: "flex", alignItems: "center", gap: 10 } }, [
        el("span", { style: { width: 36, height: 36, flex: "0 0 36px", borderRadius: 10, display: "grid", placeItems: "center", color: "var(--axon-orange-strong)", background: tint("var(--axon-orange)", 14, "var(--axon-page)"), border: `1px solid ${tint("var(--axon-orange)", 30)}` } }, Icon(op.icon, 18)),
        el("div", null, [
          el("div", { style: { fontFamily: "var(--font-display)", fontWeight: 800, fontSize: 15, color: "var(--aurora-text-primary)" } }, "Job accepted"),
          Mono(`${data.status || "pending"} · async`, "var(--axon-orange-strong)"),
        ]),
      ]),
      el("div", { style: { display: "flex", flexDirection: "column", gap: 6 } }, rows.map(([k, v]) => el("div", { style: { display: "flex", justifyContent: "space-between", gap: 12 } }, [Mono(k), el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 11.5, color: "var(--aurora-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 } }, String(v))]))),
      data.status_url ? Mono(`Track it in Jobs · ${data.status_url}`, null, 11) : null,
    ], { display: "flex", flexDirection: "column", gap: 12, background: tint("var(--axon-orange)", 8, "var(--axon-control)"), border: `1px solid ${tint("var(--axon-orange)", 28)}` });
  }

  /* ── diff → added / removed / changed ── */
  function DiffView(data, tones) {
    const Group = (label, items, color, sign) => (items && items.length ? Card([
      Mono(`${sign} ${label} (${items.length})`, color),
      ...items.map((t) => el("div", { style: { display: "flex", gap: 9, fontFamily: "var(--font-sans)", fontSize: 13, color: "var(--aurora-text-primary)" } }, [el("span", { style: { color, flex: "0 0 auto" } }, sign), el("span", null, t)])),
    ], { display: "flex", flexDirection: "column", gap: 8 }) : null);
    return col([
      Mono(`${hostOf(data.url_a)} → ${hostOf(data.url_b)}`),
      Group("added", data.added, "var(--aurora-success)", "+"),
      Group("removed", data.removed, "var(--aurora-error)", "−"),
      Group("changed", data.changed, "var(--aurora-warn)", "~"),
    ], 10);
  }

  /* ── brand → palette + type ── */
  function BrandPalette(data, tones) {
    return col([
      Mono(`${hostOf(data.url)} · ${data.colors.length} colors · ${data.fonts.length} faces`),
      el("div", { style: { display: "grid", gridTemplateColumns: "repeat(5, 1fr)", gap: 8 } }, data.colors.map((c) => el("div", { style: { display: "flex", flexDirection: "column", gap: 5 } }, [
        el("div", { style: { height: 46, borderRadius: 10, background: c.hex, border: "1px solid var(--aurora-border-default)" } }),
        Mono(c.hex, "var(--aurora-text-primary)", 9.5),
        c.role ? Mono(c.role, null, 9) : null,
      ]))),
      data.fonts.length ? Card(data.fonts.map((f) => el("div", { style: { display: "flex", justifyContent: "space-between", alignItems: "baseline" } }, [
        el("span", { style: { fontFamily: "var(--font-display)", fontSize: 15, color: "var(--aurora-text-primary)" } }, f.family),
        Mono(f.role),
      ])), { display: "flex", flexDirection: "column", gap: 8 }) : null,
    ], 11);
  }

  /* ── endpoints → discovered API surface ── */
  function EndpointsList(data, tones) {
    const kindColor = (k) => (k === "graphql" ? "#e535ab" : k === "websocket" ? "var(--axon-orange-strong)" : k === "rpc" ? "var(--aurora-accent-pink-strong)" : tones.fg);
    return col([
      Mono(`${hostOf(data.url)} · ${data.endpoints.length} endpoints · ${data.bundles.length} bundles${data.rpc_probed ? " · rpc probed" : ""}`),
      ...data.endpoints.map((e) => Card([
        el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 10, fontWeight: 700, color: "var(--aurora-text-muted)", width: 38, flex: "0 0 auto" } }, e.method),
        el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 12.5, color: "var(--aurora-text-primary)", flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } }, e.path),
        e.kind ? el("span", { style: { fontFamily: "var(--font-mono)", fontSize: 9.5, color: kindColor(e.kind), padding: "2px 7px", borderRadius: 5, border: `1px solid ${tint(kindColor(e.kind), 30)}` } }, e.kind) : null,
      ], { display: "flex", alignItems: "center", gap: 10, padding: "10px 12px" })),
    ], 8);
  }

  /* ── screenshot → captured image placeholder ── */
  function ScreenshotView(data, tones) {
    return col([
      Mono(`${hostOf(data.url)}${data.width ? ` · ${data.width}×${data.height}` : ""}${data.bytes ? ` · ${data.bytes}` : ""}${data.full_page ? " · full page" : ""}`),
      el("div", { style: { borderRadius: 12, overflow: "hidden", border: "1px solid var(--aurora-border-default)", background: "var(--axon-page)", aspectRatio: "3 / 4", maxHeight: 340, display: "grid", placeItems: "center", backgroundImage: "repeating-linear-gradient(135deg, transparent, transparent 11px, rgba(41,182,246,0.05) 11px, rgba(41,182,246,0.05) 22px)" } }, [
        el("div", { style: { display: "flex", flexDirection: "column", alignItems: "center", gap: 9, color: "var(--aurora-text-muted)" } }, [Icon("camera", 28, tones.fg), data.path ? Mono(data.path, null, 10.5) : null]),
      ]),
    ], 10);
  }

  /* ── evaluate → readable score card (real /v1/evaluate shape) ── */
  function EvaluateReport(data, tones) {
    const metrics = ["accuracy", "relevance", "completeness", "specificity"].filter((k) => data[k] != null);
    return col([
      data.verdict ? Card([
        Mono("VERDICT", tones.fg, 10.5),
        el("div", { style: { marginTop: 4, fontFamily: "var(--font-display)", fontWeight: 800, fontSize: 18, color: "var(--aurora-text-primary)", textTransform: "capitalize" } }, data.verdict),
      ]) : null,
      metrics.length ? Card(metrics.map((k) => el("div", { style: { display: "flex", alignItems: "center", gap: 10 } }, [
        el("span", { style: { fontFamily: "var(--font-sans)", fontSize: 12.5, color: "var(--aurora-text-primary)", width: 110, flex: "0 0 auto", textTransform: "capitalize" } }, k),
        el("div", { style: { flex: 1 } }, Bar(scorePct(data[k]), tones, 6)),
        Mono(String(data[k]), tones.fg),
      ])), { display: "flex", flexDirection: "column", gap: 10 }) : null,
      data.explanation || data.reasoning ? Card([el("p", { style: { margin: 0, fontFamily: "var(--font-sans)", fontSize: 13.5, lineHeight: 1.6, color: "var(--aurora-text-primary)" } }, data.explanation || data.reasoning)]) : null,
    ].filter(Boolean), 11);
  }

  /* ── ask → answer + citations ── */
  function AskAnswer(data, tones) {
    return col([
      el("div", { style: { display: "flex", gap: 11, alignItems: "flex-start" } }, [
        el("span", { style: { width: 28, height: 28, flex: "0 0 28px", borderRadius: 8, display: "grid", placeItems: "center", background: tint(tones.base, 14, "var(--axon-control)"), border: `1px solid ${tint(tones.base, 30)}` } }, window.AxonIcons.axonMark(18)),
        el("div", { style: { minWidth: 0, display: "flex", flexDirection: "column", gap: 10 } }, [
          el("p", { style: { margin: 0, fontFamily: "var(--font-sans)", fontSize: 14, lineHeight: 1.62, color: "var(--aurora-text-primary)", overflowWrap: "anywhere" } }, data.answer),
          data.citations && data.citations.length ? el("div", { style: { display: "flex", flexWrap: "wrap", gap: 7 } }, data.citations.map((c, i) => el("span", { style: { display: "inline-flex", alignItems: "center", gap: 6, fontFamily: "var(--font-mono)", fontSize: 10.5, color: tones.fg, padding: "3px 9px", borderRadius: 999, background: tint(tones.base, 10, "var(--axon-control)"), border: `1px solid ${tint(tones.base, 26)}` } }, [
            el("span", { style: { fontWeight: 600 } }, String(c.n != null ? c.n : i + 1)),
            el("span", { style: { color: "var(--aurora-text-muted)" } }, c.src || c),
          ]))) : null,
        ]),
      ]),
    ], 10);
  }

  /* ── generic fallback — readable JSON ── */
  function GenericResult(op, raw) {
    let text;
    try { text = JSON.stringify(raw, null, 2); } catch { text = String(raw); }
    if (text && text.length > 6000) text = `${text.slice(0, 6000)}\n…[truncated]`;
    return col([
      Mono(`${op.name} · response`),
      el("div", { style: { borderRadius: 11, overflow: "hidden", border: "1px solid var(--aurora-border-default)", background: "var(--axon-page)" } }, [
        el("pre", { style: { margin: 0, padding: "12px 14px", overflowX: "auto", fontFamily: "var(--font-mono)", fontSize: 12, lineHeight: 1.55, color: "var(--aurora-text-primary)", whiteSpace: "pre-wrap", overflowWrap: "anywhere" } }, text),
      ]),
    ], 9);
  }

  /* ── helpers for normalizers ── */
  function payloadOf(r) { return r && typeof r === "object" && r.payload && typeof r.payload === "object" ? r.payload : r; }
  function arr(v) { return Array.isArray(v) ? v : []; }
  function num(v) { return v == null ? "—" : Number(v).toLocaleString(); }
  function scorePct(v) { const n = Number(v); if (!Number.isFinite(n)) return 0; return n <= 1 ? n * 100 : n; }
  function markdownToBlocks(md, title) {
    const blocks = [];
    if (title) blocks.push({ t: "h1", x: title });
    const lines = String(md || "").split("\n");
    let para = [];
    let code = null;
    const flush = () => { if (para.length) { blocks.push({ t: "p", x: para.join(" ").trim() }); para = []; } };
    for (const line of lines) {
      const fence = line.match(/^```(\w*)/);
      if (fence) { if (code) { blocks.push({ t: "code", lang: code.lang, x: code.body.join("\n") }); code = null; } else { flush(); code = { lang: fence[1] || "text", body: [] }; } continue; }
      if (code) { code.body.push(line); continue; }
      const h = line.match(/^(#{1,6})\s+(.*)$/);
      if (h) { flush(); blocks.push({ t: h[1].length === 1 ? "h1" : "h2", x: h[2].trim() }); continue; }
      if (!line.trim()) { flush(); continue; }
      para.push(line.replace(/^[-*]\s+/, "").trim());
    }
    if (code) blocks.push({ t: "code", lang: code.lang, x: code.body.join("\n") });
    flush();
    return blocks.slice(0, 80);
  }

  /* ── normalizers: real /v1/* response → render data (null = fall back) ── */
  const PREP = {
    query(r) {
      const hits = arr(r.results).length ? r.results : arr(r.hits).length ? r.hits : arr(payloadOf(r).results);
      if (!hits.length) return null;
      return { results: hits.map((h, i) => {
        const p = h.payload || h;
        return {
          rank: h.rank || i + 1,
          score: h.score != null ? h.score : h.similarity != null ? h.similarity : p.score || 0,
          rerank_score: h.rerank_score != null ? h.rerank_score : h.score != null ? h.score : 0,
          url: p.url || p.source_url || h.url || "",
          snippet: p.chunk_text || p.text || p.content || p.snippet || p.title || "",
          chunk_index: p.chunk_index != null ? p.chunk_index : h.chunk_index != null ? h.chunk_index : 0,
        };
      }) };
    },
    search(r) {
      const p = payloadOf(r);
      const results = arr(p.results).length ? p.results : arr(r.results);
      const jobs = p.auto_crawl_status || (arr(p.crawl_jobs).length ? { enqueued: p.crawl_jobs.length, skipped: 0 } : null);
      return { results: results.map((x) => ({ title: x.title || x.url || x.href || "Result", url: x.url || x.href || "", snippet: x.snippet || x.content || x.text || "", score: x.score })), auto_crawl_status: jobs };
    },
    research(r) {
      const p = payloadOf(r);
      const results = arr(p.search_results).length ? p.search_results : arr(p.results).length ? p.results : arr(r.results);
      return { summary: p.summary || p.answer || p.synthesis || "", results: results.map((x) => ({ title: x.title || x.url || "Result", url: x.url || x.href || "", snippet: x.snippet || x.content || "", score: x.score })) };
    },
    summarize(r) {
      const p = payloadOf(r);
      const summary = p.summary || r.summary;
      if (!summary) return null;
      return { summary, urls: arr(p.urls || r.urls), context_chars: p.context_chars != null ? p.context_chars : r.context_chars, context_truncated: p.context_truncated || r.context_truncated };
    },
    map(r) {
      const p = payloadOf(r);
      const urls = arr(p.urls).length ? p.urls : arr(r.urls);
      return { urls: urls.slice(0, 200), total: p.total != null ? p.total : r.total != null ? r.total : urls.length, mapped_urls: p.mapped_urls != null ? p.mapped_urls : urls.length };
    },
    sources(r) {
      const p = payloadOf(r);
      const raw = arr(p.sources).length ? p.sources : arr(p.urls).length ? p.urls : arr(r.sources);
      if (!raw.length) return null;
      const urls = raw.map((s) => {
        if (Array.isArray(s)) return [s[0], s[1] || 0];
        if (typeof s === "string") return [s, 0];
        return [s.url || s.source_url || s.id || "", s.chunk_count != null ? s.chunk_count : s.chunks != null ? s.chunks : s.points || 0];
      });
      return { count: p.count != null ? p.count : urls.length, urls };
    },
    domains(r) {
      const p = payloadOf(r);
      const raw = arr(p.domains).length ? p.domains : arr(r.domains);
      if (!raw.length) return null;
      return { domains: raw.map((d) => (typeof d === "string" ? { domain: d, vectors: 0 } : { domain: d.domain || d.host || d.name || "", vectors: d.vectors != null ? d.vectors : d.chunk_count != null ? d.chunk_count : d.chunks || 0 })) };
    },
    retrieve(r) {
      const p = payloadOf(r);
      const chunks = arr(p.chunks).length ? p.chunks : arr(p.points).length ? p.points : arr(r.chunks);
      const matched = p.matched_url || r.matched_url || p.requested_url || r.requested_url || "";
      let blocks = [];
      chunks.slice(0, 12).forEach((c) => {
        const cp = c.payload || c;
        const txt = cp.text || cp.chunk_text || cp.content || cp.markdown || "";
        blocks = blocks.concat(markdownToBlocks(txt));
      });
      if (!blocks.length) return null;
      return {
        matched_url: matched,
        chunk_count: p.chunk_count != null ? p.chunk_count : chunks.length,
        token_estimate: p.token_estimate != null ? p.token_estimate : 0,
        truncated: !!(p.truncated || r.truncated),
        refresh_status: p.refresh_status || "fresh",
        blocks: blocks.slice(0, 80),
      };
    },
    stats(r) {
      const p = payloadOf(r);
      if (!p || typeof p !== "object") return null;
      return { payload: {
        documents: p.documents != null ? p.documents : p.document_count,
        chunks: p.chunks != null ? p.chunks : p.chunk_count,
        vectors: p.vectors != null ? p.vectors : p.vector_count != null ? p.vector_count : p.points_count,
        domains: p.domains != null ? p.domains : p.domain_count,
        dim: p.dim != null ? p.dim : p.dimension,
        storage_bytes: p.storage_bytes != null ? p.storage_bytes : p.bytes,
        collection: p.collection || p.collection_name,
        mode: p.mode || p.vector_mode,
        embedding_model: p.embedding_model || p.model,
        last_indexed: p.last_indexed,
      } };
    },
    doctor(r) {
      const p = payloadOf(r);
      const rawServices = p.services;
      let services = [];
      if (Array.isArray(rawServices)) {
        services = rawServices.map((s) => ({ name: s.name || s.id, status: s.status || (s.ok ? "ok" : "fail"), detail: s.detail, ms: s.ms != null ? s.ms : null }));
      } else if (rawServices && typeof rawServices === "object") {
        services = Object.entries(rawServices).map(([name, s]) => ({ name: name.replace(/_/g, " ").replace(/\b\w/g, (m) => m.toUpperCase()), status: s && s.ok ? "ok" : s && s.ok === false ? "fail" : "warn", detail: s && s.detail, ms: s && s.ms != null ? s.ms : null }));
      } else return null;
      return { payload: { ok: p.all_ok != null ? p.all_ok : p.ok, services } };
    },
    status(r) {
      const p = payloadOf(r);
      const byType = {};
      let running = 0; let pending = 0;
      const groups = Object.entries(p).filter(([k, v]) => k.endsWith("_jobs") && Array.isArray(v));
      if (groups.length) {
        groups.forEach(([k, jobs]) => {
          const kind = k.replace(/^local_/, "").replace(/_jobs$/, "");
          let run = 0; let pend = 0;
          jobs.forEach((j) => { const st = String(j.status || (j.finished_at ? "completed" : j.started_at ? "running" : "pending")).toLowerCase(); if (st === "running") run += 1; else if (st === "pending" || st === "queued") pend += 1; });
          byType[kind] = { running: run, pending: pend }; running += run; pending += pend;
        });
        return { payload: { running, pending, by_type: byType } };
      }
      if (p.running != null || p.pending != null) return { payload: { running: p.running || 0, pending: p.pending || 0, by_type: p.by_type || {} } };
      return null;
    },
    suggest(r) {
      const p = payloadOf(r);
      const raw = arr(p.suggestions).length ? p.suggestions : arr(p.urls).length ? p.urls : arr(r.suggestions);
      if (!raw.length) return null;
      return { urls: raw.map((s) => (typeof s === "string" ? { url: s, reason: "" } : { url: s.url || s.href || "", reason: s.reason || s.title || "" })) };
    },
    job(r) {
      const p = payloadOf(r);
      return { job_id: r.job_id || r.jobId || r.id || p.job_id || p.id || "", status: r.status || p.status || "pending", status_url: r.status_url || p.status_url, target: r.target || p.target, source_type: r.source_type || p.source_type };
    },
    diff(r) {
      const p = payloadOf(r);
      if (!p.added && !p.removed && !p.changed) return null;
      return { url_a: p.url_a || r.url_a || "", url_b: p.url_b || r.url_b || "", added: arr(p.added), removed: arr(p.removed), changed: arr(p.changed) };
    },
    brand(r) {
      const p = payloadOf(r);
      const colors = arr(p.colors).map((c) => (typeof c === "string" ? { hex: c } : { hex: c.hex || c.value || c.color, role: c.role || c.name }));
      if (!colors.length) return null;
      return { url: p.url || r.url || "", colors: colors.slice(0, 10), fonts: arr(p.fonts).map((f) => (typeof f === "string" ? { family: f } : { family: f.family || f.name, role: f.role || "" })) };
    },
    endpoints(r) {
      const p = payloadOf(r);
      const eps = arr(p.endpoints);
      if (!eps.length) return null;
      return { url: p.url || r.url || "", endpoints: eps.map((e) => ({ method: e.method || "GET", path: e.path || e.url || "", kind: e.kind || e.type || "rest" })), bundles: arr(p.bundles), rpc_probed: !!p.rpc_probed };
    },
    screenshot(r) {
      const p = payloadOf(r);
      return { url: p.url || r.url || "", width: p.width, height: p.height, bytes: p.bytes, full_page: p.full_page, path: p.path || p.screenshot_path };
    },
    evaluate(r) { const p = payloadOf(r); return p && typeof p === "object" ? p : null; },
    ask(r) {
      const p = payloadOf(r);
      const answer = r.answer || p.answer || "";
      if (!answer) return null;
      let citations = arr(r.citations).length ? r.citations : arr(p.citations).length ? p.citations : arr(p.sources);
      citations = citations.map((c, i) => (typeof c === "string" ? { n: i + 1, src: hostOf(c) } : { n: c.n != null ? c.n : i + 1, src: c.src || c.source || hostOf(c.url || "") }));
      return { answer, citations };
    },
    scrape(r) {
      const p = payloadOf(r);
      const md = r.markdown || p.markdown || r.content || p.content || r.output || "";
      const title = r.title || p.title;
      const blocks = md ? markdownToBlocks(md, title) : (title ? [{ t: "h1", x: title }] : []);
      if (!blocks.length) return null;
      return { blocks };
    },
  };

  /* ── dispatch ── */
  function ActionBody(op, raw, tones, handlers) {
    handlers = handlers || {};
    try {
      const id = op.id;
      if (id === "query") { const d = PREP.query(raw); return d ? QueryHits(d, tones) : GenericResult(op, raw); }
      if (id === "search") { return WebResults(PREP.search(raw), tones, "search"); }
      if (id === "research") { return WebResults(PREP.research(raw), tones, "research"); }
      if (id === "summarize") { const d = PREP.summarize(raw); return d ? SummaryCard(d, tones) : GenericResult(op, raw); }
      if (id === "map") { return MapUrls(PREP.map(raw), tones); }
      if (id === "sources") { const d = PREP.sources(raw); return d ? SourcesList(d, tones, handlers.onOpenDoc) : GenericResult(op, raw); }
      if (id === "retrieve") { const d = PREP.retrieve(raw); return d ? RetrieveDoc(d, tones) : GenericResult(op, raw); }
      if (id === "stats") { const d = PREP.stats(raw); return d ? StatsGrid(d, tones) : GenericResult(op, raw); }
      if (id === "domains") { const d = PREP.domains(raw); return d ? DomainsList(d, tones) : GenericResult(op, raw); }
      if (id === "suggest") { const d = PREP.suggest(raw); return d ? SuggestList(d, tones, handlers.onAct) : GenericResult(op, raw); }
      if (id === "doctor") { const d = PREP.doctor(raw); return d ? DoctorReport(d, tones) : GenericResult(op, raw); }
      if (id === "status") { const d = PREP.status(raw); return d ? StatusReport(d, tones) : GenericResult(op, raw); }
      if (id === "ingest" || id === "embed" || id === "extract" || id === "crawl") { return JobAccepted(PREP.job(raw), op, tones); }
      if (id === "diff") { const d = PREP.diff(raw); return d ? DiffView(d, tones) : GenericResult(op, raw); }
      if (id === "brand") { const d = PREP.brand(raw); return d ? BrandPalette(d, tones) : GenericResult(op, raw); }
      if (id === "endpoints") { const d = PREP.endpoints(raw); return d ? EndpointsList(d, tones) : GenericResult(op, raw); }
      if (id === "screenshot") { return ScreenshotView(PREP.screenshot(raw), tones); }
      if (id === "evaluate") { const d = PREP.evaluate(raw); return d ? EvaluateReport(d, tones) : GenericResult(op, raw); }
      if (id === "ask") { const d = PREP.ask(raw); return d ? AskAnswer(d, tones) : GenericResult(op, raw); }
      if (id === "scrape") { const d = PREP.scrape(raw); return d ? MdBlocks(d.blocks, tones) : GenericResult(op, raw); }
      return GenericResult(op, raw);
    } catch (e) {
      return GenericResult(op, raw);
    }
  }

  /* Build a doc-viewer body from a raw /v1/retrieve response. */
  function RetrieveDocFromRaw(raw, tones) {
    const d = PREP.retrieve(raw);
    return d ? RetrieveDoc(d, tones) : GenericResult({ id: "retrieve", name: "Document" }, raw);
  }

  window.AxonRender = { el, Icon, ActionBody, RetrieveDocFromRaw, GenericResult };
})();
