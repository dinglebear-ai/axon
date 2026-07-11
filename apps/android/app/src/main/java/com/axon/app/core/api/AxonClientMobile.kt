package com.axon.app.core.api

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import com.axon.app.core.api.models.MobileSessionDto

// ── Mobile sessions ──────────────────────────────────────────────────────
// Extension functions (not class members) so AxonClient.kt stays under the
// repo's monolith line cap. All four route through [AxonClient.generatedApi]
// (the OpenAPI-generated client), so there's nothing here beyond dispatch.

suspend fun AxonClient.listMobileSessions(): Result<List<MobileSessionDto>> = withContext(Dispatchers.IO) {
    generatedApi.listMobileSessions()
}

suspend fun AxonClient.getMobileSession(id: String): Result<MobileSessionDto> = withContext(Dispatchers.IO) {
    generatedApi.getMobileSession(id)
}

suspend fun AxonClient.upsertMobileSession(session: MobileSessionDto): Result<MobileSessionDto> = withContext(Dispatchers.IO) {
    generatedApi.upsertMobileSession(session)
}

suspend fun AxonClient.deleteMobileSession(id: String): Result<Boolean> = withContext(Dispatchers.IO) {
    generatedApi.deleteMobileSession(id)
}
