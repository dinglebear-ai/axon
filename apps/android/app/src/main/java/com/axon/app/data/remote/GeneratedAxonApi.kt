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
    suspend fun collections(): Result<PanelCollectionsResponse> = runCatching {
        val snapshot = snapshotProvider()
        val headers = snapshot.authHeaders()

        check(COLLECTIONS_OPENAPI_ROUTE == "GET /v1/collections")
        generatedClient(snapshot.baseUrl, headers)
            .collectionsOpenapiMarker()
            .toAppModel()
    }.recoverCatching { error ->
        if (error is CancellationException) throw error
        throw error.toAppFailure()
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

    private fun Throwable.toAppFailure(): Throwable =
        when (this) {
            is ClientException -> IllegalStateException(
                redactedHttpError(
                    statusCode,
                    (response as? ClientError<*>)?.body?.toString(),
                    message.orEmpty(),
                )
            )
            is ServerException -> IllegalStateException(
                redactedHttpError(
                    statusCode,
                    (response as? ServerError<*>)?.body?.toString(),
                    message.orEmpty(),
                )
            )
            else -> this
        }

    private fun redactedHttpError(code: Int, body: String?, message: String): String =
        httpErrorMessage(code, body?.redactSensitiveTokens(), message).redactSensitiveTokens()

    private fun GeneratedPanelCollectionsResponse.toAppModel(): PanelCollectionsResponse =
        PanelCollectionsResponse(collections = collections)

    private fun String.redactSensitiveTokens(): String =
        replace(Regex("(?i)(authorization|x-api-key|x-axon-panel-token)\\s*[:=]\\s*(bearer\\s+)?[^,}\\s]+"), "$1:<redacted>")
            .replace(Regex("(?i)(\"(?:access_)?token\"\\s*:\\s*\")[^\"]+(\")"), "$1<redacted>$2")
            .replace(Regex("(?i)(token=)[^,}\\s]+"), "$1<redacted>")
            .replace(Regex("secret-[A-Za-z0-9._-]*"), "<redacted>")
            .replace("secret-token", "<redacted>")
}
