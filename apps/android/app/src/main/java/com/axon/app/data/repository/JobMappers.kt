package com.axon.app.data.repository

import com.axon.app.data.remote.AxonClient
import com.axon.app.data.remote.models.ServiceJob

internal fun ServiceJob.toJobUi(kind: AxonClient.JobKind): JobUi =
    JobUi(
        kind = kind,
        id = id,
        status = status,
        url = url,
        sourceType = sourceType,
        target = target,
        errorText = errorText,
        resultJson = resultJson,
        finishedAt = finishedAt,
    )
