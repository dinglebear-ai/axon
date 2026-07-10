/* Axon launcher response normalizers. */
(function () {
  const { hostOf } = window.AxonData;

  function payloadOf(r) {
    return r && typeof r === "object" && r.payload && typeof r.payload === "object" ? r.payload : r;
  }
  function arr(v) { return Array.isArray(v) ? v : []; }
  function markdownToBlocks(md, title) {
    const blocks = [];
    const lines = String(md || "").split("\n");
    const firstHeading = lines.map((line) => line.trim()).find(Boolean)?.replace(/^#+\s*/, "");
    if (title && firstHeading !== title) blocks.push({ t: "h1", x: title });
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

  window.AxonPrep = {
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
})();
