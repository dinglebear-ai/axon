package com.axon.app.feature.ask

import androidx.lifecycle.viewModelScope
import com.axon.app.data.repository.JobFamily
import com.axon.app.ui.fab.FabOp
import kotlinx.coroutines.launch

internal fun AskViewModel.submitFabOperation(
    op: FabOp,
    input: String,
) {
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
                    onFailure = { e -> replaceLastAxonMsg(userFacingOperationError(op, e.message ?: "Unknown scrape error")) },
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
                    onFailure = { e -> appendItem(ChatItem.AxonMsg(userFacingOperationError(op, e.message ?: "Unknown extract error"))) },
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
                                endpoint = "POST /v1/sources",
                                detail = "Embed is queued. Chunks, document count, and errors are tracked in Jobs.",
                            ),
                        )
                        appendOperationContext(
                            op = FabOp.Embed,
                            target = input,
                            status = "Queued",
                            endpoint = "POST /v1/sources",
                            jobId = jobId,
                            detail = "Embedding was submitted from Axon mobile.",
                        )
                        pollJobOnce(JobFamily.Source, jobId)
                    },
                    onFailure = { e -> appendItem(ChatItem.AxonMsg(userFacingOperationError(op, e.message ?: "Unknown embed error"))) },
                )
            }

            FabOp.Research -> {
                appendOperationRequest(FabOp.Research, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.research(query = input).fold(
                    onSuccess = { r -> replaceLastAxonMsg(previewText(r.summary ?: "(no summary returned)")) },
                    onFailure = { e -> replaceLastAxonMsg(userFacingOperationError(op, e.message ?: "Unknown research error")) },
                )
            }

            FabOp.Query -> {
                appendOperationRequest(FabOp.Query, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.query(query = input).fold(
                    onSuccess = { hits ->
                        val text =
                            hits
                                .take(COMPACT_HIT_LIMIT)
                                .joinToString("\n\n") { h ->
                                    "• ${compactSingleLine(
                                        h.url,
                                        COMPACT_HIT_URL_CHARS,
                                    )}\n  ${compactSingleLine(h.snippet, COMPACT_HIT_SNIPPET_CHARS)}"
                                }.ifBlank {
                                    "No indexed results found. Try Chat for a general answer, or use Search, Index site, or Source to add material first."
                                }
                        val countNote = compactHitCountNote(hits.size)
                        replaceLastAxonMsg(text + countNote)
                    },
                    onFailure = { e -> replaceLastAxonMsg(userFacingOperationError(op, e.message ?: "Unknown query error")) },
                )
            }

            FabOp.Search -> {
                appendOperationRequest(FabOp.Search, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.searchWeb(query = input).fold(
                    onSuccess = { r ->
                        val resultsText =
                            r.results
                                .take(COMPACT_HIT_LIMIT)
                                .joinToString("\n\n") { h ->
                                    "• ${compactSingleLine(h.title, COMPACT_HIT_TITLE_CHARS)}\n" +
                                        "  ${compactSingleLine(h.url, COMPACT_HIT_URL_CHARS)}\n" +
                                        "  ${compactSingleLine(h.snippet.orEmpty(), COMPACT_HIT_SNIPPET_CHARS)}"
                                }.ifBlank { "No web results found. Try a broader query or paste a known URL into Scrape or Index site." }
                        val countNote = compactHitCountNote(r.results.size)
                        val jobsText =
                            r.sourceJobs
                                .takeIf { it.isNotEmpty() }
                                ?.let { jobs ->
                                    "\n\nQueued ${jobs.size} source jobs from search. Open Jobs for IDs and progress."
                                }.orEmpty()
                        r.sourceJobs.forEach { job ->
                            recordRecentJob(job.jobId, kind = "source", target = job.url)
                            appendOperationContext(
                                op = FabOp.SourceSite,
                                target = job.url,
                                status = "Queued",
                                endpoint = "POST /v1/search",
                                jobId = job.jobId,
                                detail = "Search auto-enqueued a source job for this result.",
                            )
                        }
                        replaceLastAxonMsg(resultsText + countNote + jobsText)
                    },
                    onFailure = { e -> replaceLastAxonMsg(userFacingOperationError(op, e.message ?: "Unknown search error")) },
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
                    onFailure = { e -> replaceLastAxonMsg(userFacingOperationError(op, e.message ?: "Unknown map error")) },
                )
            }

            FabOp.Retrieve -> {
                appendOperationRequest(FabOp.Retrieve, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.retrieve(url = input).fold(
                    onSuccess = { r -> replaceLastAxonMsg(previewText(r.content)) },
                    onFailure = { e -> replaceLastAxonMsg(userFacingOperationError(op, e.message ?: "Unknown retrieve error")) },
                )
            }

            FabOp.Summarize -> {
                appendOperationRequest(FabOp.Summarize, input)
                appendItem(ChatItem.AxonMsg("", isStreaming = true))
                repo.summarize(urls = listOf(input)).fold(
                    onSuccess = { r -> replaceLastAxonMsg(previewText(r.summary)) },
                    onFailure = { e -> replaceLastAxonMsg(userFacingOperationError(op, e.message ?: "Unknown summarize error")) },
                )
            }

            FabOp.SourceSite -> {
                appendOperationRequest(FabOp.SourceSite, input)
                repo.sourceSiteSubmit(url = input).fold(
                    onSuccess = { jobId ->
                        recordRecentJob(jobId, kind = "source", target = input)
                        appendItem(
                            ChatItem.Injection(
                                op = FabOp.SourceSite,
                                target = input,
                                jobId = jobId,
                                endpoint = "POST /v1/sources",
                                detail = "Site indexing queued. Pages, errors, and completion state are pulled from the job endpoint.",
                            ),
                        )
                        appendOperationContext(
                            op = FabOp.SourceSite,
                            target = input,
                            status = "Queued",
                            endpoint = "POST /v1/sources",
                            jobId = jobId,
                            detail = "Site source was submitted from Axon mobile. Job status and indexed pages are available from Jobs.",
                        )
                        pollSourceJobOnce(jobId)
                    },
                    onFailure = { e ->
                        appendItem(
                            ChatItem.Injection(
                                op = FabOp.SourceSite,
                                target = input,
                                jobId = null,
                                status = "FAILED",
                                endpoint = "POST /v1/sources",
                                detail = "Site indexing failed: ${e.message ?: "unknown server error"}",
                            ),
                        )
                        appendOperationContext(
                            op = FabOp.SourceSite,
                            target = input,
                            status = "FAILED",
                            endpoint = "POST /v1/sources",
                            detail = "Site indexing failed: ${e.message ?: "unknown server error"}",
                        )
                    },
                )
            }

            FabOp.Source -> {
                appendOperationRequest(FabOp.Source, input)
                repo.sourceSubmit(target = input).fold(
                    onSuccess = { jobId ->
                        recordRecentJob(jobId, kind = "source", target = input)
                        appendItem(
                            ChatItem.Injection(
                                op = FabOp.Source,
                                target = input,
                                jobId = jobId,
                                endpoint = "POST /v1/sources",
                                detail = "Source queued. Discovery, source processing, and metadata progress are tracked in Jobs.",
                            ),
                        )
                        appendOperationContext(
                            op = FabOp.Source,
                            target = input,
                            status = "Queued",
                            endpoint = "POST /v1/sources",
                            jobId = jobId,
                            detail = "Source was submitted from Axon mobile.",
                        )
                        pollJobOnce(JobFamily.Source, jobId)
                    },
                    onFailure = { e -> appendItem(ChatItem.AxonMsg(userFacingOperationError(op, e.message ?: "Unknown source error"))) },
                )
            }
        }
    }
}

internal fun compactHitCountNote(total: Int): String =
    if (total > COMPACT_HIT_LIMIT) "\n\nShowing $COMPACT_HIT_LIMIT of $total results." else ""

internal fun userFacingOperationError(
    op: FabOp,
    message: String,
): String {
    val detail = message.trim().ifBlank { "Unknown error" }
    val hint =
        when (op) {
            FabOp.Scrape, FabOp.Map, FabOp.Retrieve, FabOp.Summarize -> {
                "Check that the URL is reachable from the Axon server, then retry."
            }

            FabOp.SourceSite -> {
                "Check the site URL and server status, then retry or open Jobs for any queued work."
            }

            FabOp.Search, FabOp.Research, FabOp.Query -> {
                "Check connection/auth status and try a narrower query."
            }

            FabOp.Embed -> {
                "Check that the input is a URL or server-readable source, then retry."
            }

            FabOp.Extract -> {
                "Check the target URL and extraction options, then retry."
            }

            FabOp.Source -> {
                "Check the source target format, then retry."
            }
        }
    return "Error: ${op.label} could not complete. $hint\n\nDetail: $detail"
}
