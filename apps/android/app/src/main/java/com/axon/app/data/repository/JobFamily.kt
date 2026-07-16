package com.axon.app.data.repository

/**
 * Domain-layer job type. UI code uses [JobFamily]; wire routing details
 * ([AxonClient.JobKind] and its [path] field) stay inside [AxonRepository].
 */
enum class JobFamily {
    Source, Extract;

    fun label(): String = when (this) {
        Source -> "Source"
        Extract -> "Extract"
    }

    fun drillTitle(): String = when (this) {
        Source -> "Sources"
        Extract -> "Extractions"
    }
}
