package com.axon.app.data.repository

import com.axon.app.core.api.AxonClient
import com.axon.app.core.api.models.ServiceJob

/** Wire enum → domain enum. Used at the repository boundary. */
internal fun AxonClient.JobKind.toJobFamily(): JobFamily =
    when (this) {
        AxonClient.JobKind.Source -> JobFamily.Source
        AxonClient.JobKind.Extract -> JobFamily.Extract
    }

/** Domain enum → wire enum. Used at the repository boundary. */
internal fun JobFamily.toClientKind(): AxonClient.JobKind =
    when (this) {
        JobFamily.Source -> AxonClient.JobKind.Source
        JobFamily.Extract -> AxonClient.JobKind.Extract
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
        sourceKind = sourceKind,
        target = target,
        errorText = errorText,
        progressJson = progressJson,
        resultJson = resultJson,
        configJson = configJson,
    )
