package com.axon.app.data.remote

import com.axon.app.data.remote.models.PanelCollectionsResponse
import com.axon.app.generated.api.DiscoveryApi
import com.axon.app.generated.model.PanelCollectionsResponse as GeneratedPanelCollectionsResponse
import kotlinx.coroutines.CancellationException
import org.openapitools.client.infrastructure.ClientError
import org.openapitools.client.infrastructure.ClientException
import org.openapitools.client.infrastructure.ServerError
import org.openapitools.client.infrastructure.ServerException

private const val COLLECTIONS_OPENAPI_ROUTE = "GET /v1/collections"

internal class GeneratedAxonApi(
    private val snapshotProvider: () -> ClientAuthSnapshot,
    private val clients: AxonHttpClients,
) {
    suspend fun collections(): Result<PanelCollectionsResponse> {
        var sensitiveValues = emptyList<String>()
        return runCatching {
            val snapshot = snapshotProvider()
            val headers = snapshot.authHeaders()
            sensitiveValues = headers.sensitiveHeaderValues()

            check(COLLECTIONS_OPENAPI_ROUTE == "GET /v1/collections")
            generatedClient(snapshot.baseUrl, headers)
                .collectionsOpenapiMarker()
                .toAppModel()
        }.recoverCatching { error ->
            if (error is CancellationException) throw error
            throw error.toAppFailure(sensitiveValues)
        }
    }

    private fun generatedClient(baseUrl: String, headers: Map<String, String>): DiscoveryApi {
        val authenticated = clients.normal.newBuilder()
            .addInterceptor { chain ->
                val request = chain.request().newBuilder()
                headers.forEach { (name, value) -> request.header(name, value) }
                chain.proceed(request.build())
            }
            .build()

        return DiscoveryApi(basePath = baseUrl, client = authenticated)
    }

    private fun Throwable.toAppFailure(sensitiveValues: Collection<String>): Throwable =
        when (this) {
            is ClientException -> IllegalStateException(
                redactedHttpError(
                    statusCode,
                    (response as? ClientError<*>)?.body?.toString(),
                    message.orEmpty(),
                    sensitiveValues,
                )
            )
            is ServerException -> IllegalStateException(
                redactedHttpError(
                    statusCode,
                    (response as? ServerError<*>)?.body?.toString(),
                    message.orEmpty(),
                    sensitiveValues,
                )
            )
            else -> this
        }

    private fun redactedHttpError(
        code: Int,
        body: String?,
        message: String,
        sensitiveValues: Collection<String>,
    ): String =
        httpErrorMessage(
            code,
            body?.redactSensitiveTokens(sensitiveValues),
            message,
        ).redactSensitiveTokens(sensitiveValues)

    private fun GeneratedPanelCollectionsResponse.toAppModel(): PanelCollectionsResponse =
        PanelCollectionsResponse(collections = collections)

    private fun Map<String, String>.sensitiveHeaderValues(): List<String> =
        values.flatMap { value ->
            listOf(value, value.removePrefix("Bearer ").removePrefix("bearer "))
        }
            .filter { it.isNotBlank() }
            .distinct()
            .sortedByDescending(String::length)

    private fun String.redactSensitiveTokens(sensitiveValues: Collection<String>): String =
        sensitiveValues.fold(this) { redacted, value ->
            redacted.replace(value, "<redacted>")
        }
            .replace(Regex("(?i)(authorization|x-api-key|x-axon-panel-token)\\s*[:=]\\s*(bearer\\s+)?[^,}\\s]+"), "$1:<redacted>")
            .replace(Regex("(?i)(\"(?:access_)?token\"\\s*:\\s*\")[^\"]+(\")"), "$1<redacted>$2")
            .replace(Regex("(?i)(token=)[^,}\\s]+"), "$1<redacted>")
            .replace(Regex("secret-[A-Za-z0-9._-]*"), "<redacted>")
}
