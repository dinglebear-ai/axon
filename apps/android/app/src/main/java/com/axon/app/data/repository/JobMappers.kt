package com.axon.app.data.repository

import com.axon.app.data.remote.AxonClient
import com.axon.app.data.remote.models.ServiceJob

/** Wire enum → domain enum. Used at the repository boundary. */
internal fun AxonClient.JobKind.toJobFamily(): JobFamily = when (this) {
    AxonClient.JobKind.Crawl -> JobFamily.Crawl
    AxonClient.JobKind.Embed -> JobFamily.Embed
    AxonClient.JobKind.Extract -> JobFamily.Extract
    AxonClient.JobKind.Ingest -> JobFamily.Ingest
}

/** Domain enum → wire enum. Used at the repository boundary. */
internal fun JobFamily.toClientKind(): AxonClient.JobKind = when (this) {
    JobFamily.Crawl -> AxonClient.JobKind.Crawl
    JobFamily.Embed -> AxonClient.JobKind.Embed
    JobFamily.Extract -> AxonClient.JobKind.Extract
    JobFamily.Ingest -> AxonClient.JobKind.Ingest
}

internal fun ServiceJob.toJobUi(kind: JobFamily): JobUi =
    JobUi(
        kind = kind,
        id = id,
        status = status,
        createdAt = createdAt,
        startedAt = startedAt,
        updatedAt = updatedAt,
        finishedAt = finishedAt,
        url = url,
        sourceType = sourceType,
        target = target,
        errorText = errorText,
        progressJson = progressJson,
        resultJson = resultJson,
        configJson = configJson,
    )
