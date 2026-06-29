package com.axon.app.ui.ask

import androidx.lifecycle.viewModelScope
import com.axon.app.data.repository.JobFamily
import com.axon.app.ui.fab.FabOp
import kotlinx.coroutines.launch

internal fun AskViewModel.submitFabOperation(op: FabOp, input: String) {
    viewModelScope.launch {
        val repo = container.axonRepository
        when (op) {
            FabOp.Scrape -> {
                appendOperationRequest(op, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.scrape(url = input).fold(
                    onSuccess = { r ->
                        val summary = scrapeSummary(r.markdown)
                        replaceLastAxonItem(
                            ChatItem.ActionResult(
                                op = op,
                                target = r.url.ifBlank { input },
                                status = "200 OK",
                                endpoint = "POST /v1/scrape",
                                summary = summary,
                                body = previewText(humanMarkdownPreview(r.markdown), limit = 900),
                            ),
                        )
                        appendOperationContext(
                            op = op,
                            target = r.url.ifBlank { input },
                            status = "200 OK",
                            endpoint = "POST /v1/scrape",
                            summary = summary,
                            detail = "Scraped markdown is now available in this conversation. Use the Axon skill when reasoning over scraped or indexed content.",
                        )
                    },
                    onFailure = { e -> replaceLastAxonMsg(userFacingAskError(e.message ?: "Unknown scrape error")) },
                )
            }
            FabOp.Extract -> {
                appendOperationRequest(FabOp.Extract, input)
                repo.extractStart(url = input).fold(
                    onSuccess = { jobId ->
                        recordRecentJob(jobId, kind = "extract", target = input)
                        appendItem(
                            ChatItem.Injection(
                                op = FabOp.Extract,
                                target = input,
                                jobId = jobId,
                                endpoint = "POST /v1/extract",
                                detail = "Extraction is queued. Jobs will show schema output and any server errors.",
                            ),
                        )
                        appendOperationContext(
                            op = FabOp.Extract,
                            target = input,
                            status = "Queued",
                            endpoint = "POST /v1/extract",
                            jobId = jobId,
                            detail = "Extraction was submitted from Axon mobile.",
                        )
                        pollJobOnce(JobFamily.Extract, jobId)
                    },
                    onFailure = { e -> appendItem(ChatItem.AxonMsg(userFacingAskError(e.message ?: "Unknown extract error"))) },
                )
            }
            FabOp.Embed -> {
                appendOperationRequest(FabOp.Embed, input)
                repo.embedStart(input = input).fold(
                    onSuccess = { jobId ->
                        recordRecentJob(jobId, kind = "embed", target = input)
                        appendItem(
                            ChatItem.Injection(
                                op = FabOp.Embed,
                                target = input,
                                jobId = jobId,
                                endpoint = "POST /v1/embed",
                                detail = "Embed is queued. Chunks, document count, and errors are tracked in Jobs.",
                            ),
                        )
                        appendOperationContext(
                            op = FabOp.Embed,
                            target = input,
                            status = "Queued",
                            endpoint = "POST /v1/embed",
                            jobId = jobId,
                            detail = "Embedding was submitted from Axon mobile.",
                        )
                        pollJobOnce(JobFamily.Embed, jobId)
                    },
                    onFailure = { e -> appendItem(ChatItem.AxonMsg(userFacingAskError(e.message ?: "Unknown embed error"))) },
                )
            }
            FabOp.Research -> {
                appendOperationRequest(FabOp.Research, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.research(query = input).fold(
                    onSuccess = { r -> replaceLastAxonMsg(previewText(r.summary ?: "(no summary returned)")) },
                    onFailure = { e -> replaceLastAxonMsg(userFacingAskError(e.message ?: "Unknown research error")) },
                )
            }
            FabOp.Query -> {
                appendOperationRequest(FabOp.Query, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.query(query = input).fold(
                    onSuccess = { hits ->
                        val text = hits.take(COMPACT_HIT_LIMIT).joinToString("\n\n") { h ->
                            "• ${compactSingleLine(h.url, COMPACT_HIT_URL_CHARS)}\n  ${compactSingleLine(h.snippet, COMPACT_HIT_SNIPPET_CHARS)}"
                        }.ifBlank { "No indexed results found. Try Chat for a general answer, or use Search/Crawl/Embed to add source material first." }
                        val countNote = compactHitCountNote(hits.size)
                        replaceLastAxonMsg(text + countNote)
                    },
                    onFailure = { e -> replaceLastAxonMsg(userFacingAskError(e.message ?: "Unknown query error")) },
                )
            }
            FabOp.Search -> {
                appendOperationRequest(FabOp.Search, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.searchWeb(query = input).fold(
                    onSuccess = { r ->
                        val resultsText = r.results.take(COMPACT_HIT_LIMIT).joinToString("\n\n") { h ->
                            "• ${compactSingleLine(h.title, COMPACT_HIT_TITLE_CHARS)}\n" +
                                "  ${compactSingleLine(h.url, COMPACT_HIT_URL_CHARS)}\n" +
                                "  ${compactSingleLine(h.snippet.orEmpty(), COMPACT_HIT_SNIPPET_CHARS)}"
                        }.ifBlank { "No web results found. Try a broader query or paste a known URL into Scrape/Crawl." }
                        val countNote = compactHitCountNote(r.results.size)
                        val jobsText = r.crawlJobs.takeIf { it.isNotEmpty() }?.let { jobs ->
                            "\n\nQueued ${jobs.size} crawl jobs from search. Open Jobs for IDs and progress."
                        }.orEmpty()
                        r.crawlJobs.forEach { job ->
                            recordRecentJob(job.jobId, kind = "crawl", target = job.url)
                            appendOperationContext(
                                op = FabOp.Crawl,
                                target = job.url,
                                status = "Queued",
                                endpoint = "POST /v1/search",
                                jobId = job.jobId,
                                detail = "Search auto-enqueued a crawl job for this result.",
                            )
                        }
                        replaceLastAxonMsg(resultsText + countNote + jobsText)
                    },
                    onFailure = { e -> replaceLastAxonMsg(userFacingAskError(e.message ?: "Unknown search error")) },
                )
            }
            FabOp.Map -> {
                appendOperationRequest(FabOp.Map, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.map(url = input).fold(
                    onSuccess = { r ->
                        val text = "Found ${r.total} URLs:\n" + r.urls.take(20).joinToString("\n") { "• $it" }
                        replaceLastAxonMsg(text)
                    },
                    onFailure = { e -> replaceLastAxonMsg(userFacingAskError(e.message ?: "Unknown map error")) },
                )
            }
            FabOp.Retrieve -> {
                appendOperationRequest(FabOp.Retrieve, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.retrieve(url = input).fold(
                    onSuccess = { r -> replaceLastAxonMsg(previewText(r.content)) },
                    onFailure = { e -> replaceLastAxonMsg(userFacingAskError(e.message ?: "Unknown retrieve error")) },
                )
            }
            FabOp.Summarize -> {
                appendOperationRequest(FabOp.Summarize, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.summarize(urls = listOf(input)).fold(
                    onSuccess = { r -> replaceLastAxonMsg(previewText(r.summary)) },
                    onFailure = { e -> replaceLastAxonMsg(userFacingAskError(e.message ?: "Unknown summarize error")) },
                )
            }
            FabOp.Crawl -> {
                appendOperationRequest(FabOp.Crawl, input)
                repo.crawlSubmit(url = input).fold(
                    onSuccess = { jobId ->
                        recordRecentJob(jobId, kind = "crawl", target = input)
                        appendItem(
                            ChatItem.Injection(
                                op = FabOp.Crawl,
                                target = input,
                                jobId = jobId,
                                endpoint = "POST /v1/crawl",
                                detail = "Crawl queued. Pages, errors, and completion state are pulled from the job endpoint.",
                            ),
                        )
                        appendOperationContext(
                            op = FabOp.Crawl,
                            target = input,
                            status = "Queued",
                            endpoint = "POST /v1/crawl",
                            jobId = jobId,
                            detail = "Crawl was submitted from Axon mobile. Job status and indexed pages are available from Jobs.",
                        )
                        pollCrawlOnce(jobId)
                    },
                    onFailure = { e ->
                        appendItem(
                            ChatItem.Injection(
                                op = FabOp.Crawl,
                                target = input,
                                jobId = null,
                                status = "FAILED",
                                endpoint = "POST /v1/crawl",
                                detail = "Crawl failed: ${e.message ?: "unknown server error"}",
                            ),
                        )
                        appendOperationContext(
                            op = FabOp.Crawl,
                            target = input,
                            status = "FAILED",
                            endpoint = "POST /v1/crawl",
                            detail = "Crawl failed: ${e.message ?: "unknown server error"}",
                        )
                    },
                )
            }
            FabOp.Ingest -> {
                appendOperationRequest(FabOp.Ingest, input)
                val sourceType = inferFabIngestSource(input).fold(
                    onSuccess = { it.wire },
                    onFailure = { e ->
                        appendItem(ChatItem.AxonMsg(userFacingAskError(e.message ?: "Invalid ingest target")))
                        return@launch
                    },
                )
                repo.ingestStart(sourceType = sourceType, target = input).fold(
                    onSuccess = { jobId ->
                        recordRecentJob(jobId, kind = "ingest", target = input)
                        appendItem(
                            ChatItem.Injection(
                                op = FabOp.Ingest,
                                target = input,
                                jobId = jobId,
                                endpoint = "POST /v1/ingest",
                                detail = "Ingest queued. Discovery, source processing, and metadata progress are tracked in Jobs.",
                            ),
                        )
                        appendOperationContext(
                            op = FabOp.Ingest,
                            target = input,
                            status = "Queued",
                            endpoint = "POST /v1/ingest",
                            jobId = jobId,
                            detail = "Ingest was submitted from Axon mobile.",
                        )
                        pollJobOnce(JobFamily.Ingest, jobId)
                    },
                    onFailure = { e -> appendItem(ChatItem.AxonMsg(userFacingAskError(e.message ?: "Unknown ingest error"))) },
                )
            }
        }
    }
}

internal fun compactHitCountNote(total: Int): String =
    if (total > COMPACT_HIT_LIMIT) "\n\nShowing $COMPACT_HIT_LIMIT of $total results." else ""
