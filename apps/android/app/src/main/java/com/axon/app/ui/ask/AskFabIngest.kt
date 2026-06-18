package com.axon.app.ui.ask

import com.axon.app.data.util.UrlValidator
import com.axon.app.ui.ingest.IngestSource

internal fun inferFabIngestSource(input: String): Result<IngestSource> {
    val target = input.trim()
    if (target.isBlank()) {
        return Result.failure(IllegalArgumentException("Target is required"))
    }

    if (target.startsWith("github/", ignoreCase = true)) return Result.success(IngestSource.Github)
    if (target.startsWith("gitlab/", ignoreCase = true)) return Result.success(IngestSource.Gitlab)
    if (target.startsWith("r/", ignoreCase = true)) return Result.success(IngestSource.Reddit)

    val host = UrlValidator.hostOrNull(target)
    val source = when {
        host == null -> IngestSource.Git
        IngestSource.Github.matchesHost(host) -> IngestSource.Github
        IngestSource.Gitlab.matchesHost(host) -> IngestSource.Gitlab
        IngestSource.Reddit.matchesHost(host) -> IngestSource.Reddit
        IngestSource.Youtube.matchesHost(host) -> IngestSource.Youtube
        else -> {
            val lookalikeToken = knownIngestHostToken(host) ?: return Result.success(IngestSource.Git)
            return Result.failure(
                IllegalArgumentException("Unsupported lookalike host: $lookalikeToken must be the registrable host or a real subdomain"),
            )
        }
    }

    source.validate(target)?.let { reason ->
        return Result.failure(IllegalArgumentException(reason))
    }
    return Result.success(source)
}

private fun knownIngestHostToken(host: String): String? =
    listOf("github.com", "gitlab.com", "reddit.com", "youtube.com", "youtu.be")
        .firstOrNull { host.contains(it) }
