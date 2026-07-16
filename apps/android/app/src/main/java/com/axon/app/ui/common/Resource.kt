package com.axon.app.ui.common

/**
 * Generic UI state holder used across mode screens to avoid one-off per-feature
 * sealed interfaces. Multi-state features (for example submit/status flows)
 * keep their own bespoke sealed interface — Resource is for the common
 * "loading once, ready or error" shape.
 */
sealed interface Resource<out T> {
    data object Idle : Resource<Nothing>

    data object Loading : Resource<Nothing>

    data class Ready<out T>(
        val value: T,
    ) : Resource<T>

    data class Error(
        val message: String,
    ) : Resource<Nothing>
}
