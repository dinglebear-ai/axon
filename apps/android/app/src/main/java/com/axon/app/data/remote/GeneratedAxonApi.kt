package com.axon.app.data.remote

import com.axon.app.data.auth.AuthConfig
import com.axon.app.data.auth.MissingAuthException
import com.axon.app.data.remote.models.PanelCollectionsResponse
import com.axon.app.generated.api.DiscoveryApi
import com.axon.app.generated.model.PanelCollectionsResponse as GeneratedPanelCollectionsResponse
import kotlinx.coroutines.CancellationException
import org.openapitools.client.infrastructure.ClientError
import org.openapitools.client.infrastructure.ClientException
import org.openapitools.client.infrastructure.ServerError
import org.openapitools.client.infrastructure.ServerException

internal class GeneratedAxonApi(
    private val baseUrlProvider: () -> String,
    private val authProvider: () -> Pair<String, AuthConfig>,
    private val clients: AxonHttpClients,
) {
    suspend fun collections(): Result<PanelCollectionsResponse> = runCatching {
        val (baseUrl, auth) = authProvider()
        val headers = authHeaders(baseUrl, auth)

        // Generated surface: DiscoveryApi.collectionsOpenapiMarker() maps GET /v1/collections.
        generatedClient(baseUrl, headers)
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

    private suspend fun authHeaders(baseUrl: String, auth: AuthConfig): Map<String, String> =
        when (auth) {
            is AuthConfig.Bearer -> {
                val token = auth.token.trim()
                if (token.isBlank()) throw MissingAuthException("No Axon authentication configured")
                mapOf("Authorization" to "Bearer $token", "x-api-key" to token)
            }
            is AuthConfig.OAuth -> {
                if (auth.serverUrl.trimEnd('/') != baseUrl.trimEnd('/')) {
                    throw MissingAuthException("OAuth credentials belong to a different Axon server; sign in again for this server")
                }
                val token = auth.tokenSource.freshAccessToken().getOrThrow()
                mapOf("Authorization" to "Bearer $token")
            }
        }

    private fun Throwable.toAppFailure(): Throwable =
        when (this) {
            is ClientException -> IllegalStateException(
                redactedHttpError(
                    statusCode,
                    (response as? ClientError<*>)?.body?.toString(),
                    message.orEmpty(),
                ),
                this,
            )
            is ServerException -> IllegalStateException(
                redactedHttpError(
                    statusCode,
                    (response as? ServerError<*>)?.body?.toString(),
                    message.orEmpty(),
                ),
                this,
            )
            else -> this
        }

    private fun redactedHttpError(code: Int, body: String?, message: String): String =
        httpErrorMessage(code, body?.redactSensitiveTokens(), message).redactSensitiveTokens()

    private fun GeneratedPanelCollectionsResponse.toAppModel(): PanelCollectionsResponse =
        PanelCollectionsResponse(collections = collections)

    private fun String.redactSensitiveTokens(): String =
        replace(Regex("(?i)(authorization|x-api-key|x-axon-panel-token)[:=][^,}\\s]+"), "$1:<redacted>")
            .replace(Regex("secret-[A-Za-z0-9._-]*"), "<redacted>")
            .replace("secret-token", "<redacted>")
}
