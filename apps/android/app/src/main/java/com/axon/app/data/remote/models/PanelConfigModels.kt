package com.axon.app.data.remote.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class PanelLoginRequest(
    val password: String,
)

@Serializable
data class PanelLoginResponse(
    val ok: Boolean,
    val token: String? = null,
)

@Serializable
data class PanelConfigResponse(
    val path: String,
    @SerialName("raw_toml") val rawToml: String,
    @SerialName("restart_required") val restartRequired: Boolean,
)

@Serializable
data class PanelEnvResponse(
    val path: String,
    @SerialName("raw_env") val rawEnv: String,
    @SerialName("restart_required") val restartRequired: Boolean,
)

@Serializable
data class SavePanelConfigRequest(
    @SerialName("raw_toml") val rawToml: String,
)

@Serializable
data class SavePanelEnvRequest(
    @SerialName("raw_env") val rawEnv: String,
)

@Serializable
data class SavePanelConfigResponse(
    val ok: Boolean,
    @SerialName("restart_required") val restartRequired: Boolean,
    val message: String,
)
