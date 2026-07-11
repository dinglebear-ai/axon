package com.axon.app.core.api

import com.axon.app.core.api.models.PanelCollectionsResponse
import com.axon.app.generated.api.DiscoveryApi
import com.axon.app.generated.api.MobileApi
import com.axon.app.generated.model.PanelCollectionsResponse as GeneratedPanelCollectionsResponse
import com.axon.app.generated.model.DeleteMobileSessionResponse as GeneratedDeleteMobileSessionResponse
import com.axon.app.generated.model.MobileChatItem as GeneratedMobileChatItem
import com.axon.app.generated.model.MobileSession as GeneratedMobileSession
import com.axon.app.generated.model.MobileSessionDetailResponse as GeneratedMobileSessionDetailResponse
import com.axon.app.generated.model.MobileSessionListResponse as GeneratedMobileSessionListResponse
import com.axon.app.generated.model.MobileSessionStatus as GeneratedMobileSessionStatus
import com.axon.app.generated.model.MobileSessionSummary as GeneratedMobileSessionSummary
import com.axon.app.generated.model.UpsertMobileSessionRequest as GeneratedUpsertMobileSessionRequest
import com.axon.app.generated.model.UpsertMobileSessionResponse as GeneratedUpsertMobileSessionResponse
import com.axon.app.core.api.models.MobileChatItemDto
import com.axon.app.core.api.models.MobileSessionDto
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.longOrNull
import okhttp3.OkHttpClient
import org.openapitools.client.infrastructure.ClientError
import org.openapitools.client.infrastructure.ClientException
import org.openapitools.client.infrastructure.ServerError
import org.openapitools.client.infrastructure.ServerException

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

            openApiRoute("GET", "/v1/collections")
            generatedClient(snapshot.baseUrl, headers)
                .collectionsOpenapiMarker()
                .toAppModel()
        }.recoverCatching { error ->
            if (error is CancellationException) throw error
            throw error.toAppFailure(sensitiveValues)
        }
    }

    suspend fun listMobileSessions(): Result<List<MobileSessionDto>> =
        generatedCall { mobileClient ->
            openApiRoute("GET", "/v1/mobile/sessions")
            mobileClient.listMobileSessions().toAppModel()
        }

    suspend fun getMobileSession(id: String): Result<MobileSessionDto> =
        generatedCall { mobileClient ->
            openApiRoute("GET", "/v1/mobile/sessions/{id}", "/v1/mobile/sessions/$id")
            mobileClient.getMobileSession(id).toAppModel()
        }

    suspend fun upsertMobileSession(session: MobileSessionDto): Result<MobileSessionDto> =
        generatedCall { mobileClient ->
            openApiRoute("PUT", "/v1/mobile/sessions/{id}", "/v1/mobile/sessions/${session.id}")
            mobileClient.upsertMobileSession(
                session.id,
                GeneratedUpsertMobileSessionRequest(session.toGenerated()),
            ).toAppModel()
        }

    suspend fun deleteMobileSession(id: String): Result<Boolean> =
        generatedCall { mobileClient ->
            openApiRoute("DELETE", "/v1/mobile/sessions/{id}", "/v1/mobile/sessions/$id")
            mobileClient.deleteMobileSession(id).toAppModel()
        }

    private suspend inline fun <T> generatedCall(block: (MobileApi) -> T): Result<T> {
        var sensitiveValues = emptyList<String>()
        return runCatching {
            val snapshot = snapshotProvider()
            val headers = snapshot.authHeaders()
            sensitiveValues = headers.sensitiveHeaderValues()
            block(generatedMobileClient(snapshot.baseUrl, headers))
        }.recoverCatching { error ->
            if (error is CancellationException) throw error
            throw error.toAppFailure(sensitiveValues)
        }
    }

    private fun generatedClient(baseUrl: String, headers: Map<String, String>): DiscoveryApi {
        return DiscoveryApi(basePath = baseUrl, client = authenticatedClient(headers))
    }

    private fun generatedMobileClient(baseUrl: String, headers: Map<String, String>): MobileApi {
        return MobileApi(basePath = baseUrl, client = authenticatedClient(headers))
    }

    private fun authenticatedClient(headers: Map<String, String>): OkHttpClient =
        clients.normal.newBuilder()
            .addInterceptor { chain ->
                val request = chain.request().newBuilder()
                headers.forEach { (name, value) -> request.header(name, value) }
                chain.proceed(request.build())
            }
            .build()

    private fun openApiRoute(method: String, template: String, resolved: String = template): String {
        require(method == "GET" || method == "POST" || method == "PUT" || method == "DELETE")
        require(template.startsWith("/v1/"))
        require(resolved.startsWith("/v1/"))
        return resolved
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

    private fun GeneratedMobileSessionListResponse.toAppModel(): List<MobileSessionDto> =
        sessions.map { it.toAppModel() }

    private fun GeneratedMobileSessionDetailResponse.toAppModel(): MobileSessionDto =
        session.toAppModel()

    private fun GeneratedUpsertMobileSessionResponse.toAppModel(): MobileSessionDto =
        session.toAppModel()

    private fun GeneratedDeleteMobileSessionResponse.toAppModel(): Boolean = ok

    private fun GeneratedMobileSessionSummary.toAppModel(): MobileSessionDto =
        MobileSessionDto(
            id = id,
            title = title,
            firstMessagePreview = firstMessagePreview,
            turnCount = turnCount,
            injectedOpCount = injectedOpCount,
            createdAt = createdAt,
            updatedAt = updatedAt,
            pinnedAt = pinnedAt,
        )

    private fun GeneratedMobileSession.toAppModel(): MobileSessionDto =
        MobileSessionDto(
            id = id,
            title = title,
            firstMessagePreview = firstMessagePreview,
            turnCount = turnCount,
            injectedOpCount = injectedOpCount,
            createdAt = createdAt,
            updatedAt = updatedAt,
            pinnedAt = pinnedAt,
            items = items.orEmpty().map { it.toAppModel() },
            status = status?.value ?: "active",
            sourceRefs = sourceRefs.orEmpty(),
            draft = draft,
            syncVersion = syncVersion,
        )

    private fun GeneratedMobileChatItem.toAppModel(): MobileChatItemDto =
        MobileChatItemDto(
            kind = kind,
            text = text,
            payload = payload.toJsonObject(),
            timestamp = timestamp,
        )

    private fun MobileSessionDto.toGenerated(): GeneratedMobileSession =
        GeneratedMobileSession(
            id = id,
            title = title,
            firstMessagePreview = firstMessagePreview,
            turnCount = turnCount,
            injectedOpCount = injectedOpCount,
            createdAt = createdAt,
            updatedAt = updatedAt,
            pinnedAt = pinnedAt,
            items = items.map { it.toGenerated() },
            status = GeneratedMobileSessionStatus.decode(status) ?: GeneratedMobileSessionStatus.active,
            sourceRefs = sourceRefs,
            draft = draft,
            syncVersion = syncVersion,
        )

    private fun MobileChatItemDto.toGenerated(): GeneratedMobileChatItem =
        GeneratedMobileChatItem(
            kind = kind,
            text = text,
            payload = payload.toMoshiValue(),
            timestamp = timestamp,
        )

    private fun Any?.toJsonObject(): JsonObject =
        when (val converted = toJsonElement()) {
            is JsonObject -> converted
            else -> buildJsonObject {}
        }

    private fun Any?.toJsonElement(): JsonElement =
        when (this) {
            null -> JsonNull
            is JsonElement -> this
            is Map<*, *> -> JsonObject(
                entries
                    .filter { (key, _) -> key is String }
                    .associate { (key, value) -> key as String to value.toJsonElement() },
            )
            is List<*> -> JsonArray(map { it.toJsonElement() })
            is String -> JsonPrimitive(this)
            is Boolean -> JsonPrimitive(this)
            is Int -> JsonPrimitive(this)
            is Long -> JsonPrimitive(this)
            is Float -> JsonPrimitive(this)
            is Double -> JsonPrimitive(this)
            is Number -> JsonPrimitive(this.toDouble())
            else -> JsonPrimitive(toString())
        }

    private fun JsonElement.toMoshiValue(): Any? =
        when (this) {
            JsonNull -> null
            is JsonObject -> entries.associate { (key, value) -> key to value.toMoshiValue() }
            is JsonArray -> map { it.toMoshiValue() }
            is JsonPrimitive -> when {
                booleanOrNull != null -> booleanOrNull
                intOrNull != null -> intOrNull
                longOrNull != null -> longOrNull
                doubleOrNull != null -> doubleOrNull
                else -> contentOrNull
            }
        }

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
