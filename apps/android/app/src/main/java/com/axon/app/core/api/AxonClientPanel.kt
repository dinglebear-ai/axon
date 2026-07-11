package com.axon.app.core.api

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import com.axon.app.core.api.models.PanelCollectionsResponse
import com.axon.app.core.api.models.PanelConfigResponse
import com.axon.app.core.api.models.PanelEnvResponse
import com.axon.app.core.api.models.SavePanelConfigRequest
import com.axon.app.core.api.models.SavePanelConfigResponse
import com.axon.app.core.api.models.SavePanelEnvRequest
import java.net.URLEncoder

// ── Panel config / env / collections ────────────────────────────────────
// Extension functions (not class members) so AxonClient.kt stays under the
// repo's monolith line cap. Public call sites are unaffected.

suspend fun AxonClient.artifactText(relativePath: String): Result<String> = withContext(Dispatchers.IO) {
    val encodedPath = URLEncoder.encode(relativePath, "UTF-8").replace("+", "%20")
    getText(openApiRoute("GET", "/v1/artifacts", "/v1/artifacts?path=$encodedPath"))
}

suspend fun AxonClient.panelConfig(): Result<PanelConfigResponse> = withContext(Dispatchers.IO) {
    get("/api/panel/config")
}

suspend fun AxonClient.panelEnv(): Result<PanelEnvResponse> = withContext(Dispatchers.IO) {
    get("/api/panel/env")
}

suspend fun AxonClient.savePanelConfig(rawToml: String): Result<SavePanelConfigResponse> = withContext(Dispatchers.IO) {
    put("/api/panel/config", SavePanelConfigRequest(rawToml))
}

suspend fun AxonClient.savePanelEnv(rawEnv: String): Result<SavePanelConfigResponse> = withContext(Dispatchers.IO) {
    put("/api/panel/env", SavePanelEnvRequest(rawEnv))
}

suspend fun AxonClient.panelCollections(): Result<PanelCollectionsResponse> = withContext(Dispatchers.IO) {
    get("/api/panel/collections")
}

suspend fun AxonClient.collections(): Result<PanelCollectionsResponse> = withContext(Dispatchers.IO) {
    generatedApi.collections()
}
