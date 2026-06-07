package com.axon.app.ui.knowledge

import com.axon.app.data.repository.DomainFacetUi
import com.axon.app.data.repository.SourceEntryUi
import com.axon.app.data.repository.SuggestHitUi
import com.axon.app.ui.common.Resource
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive

internal fun suggestDetail(state: Resource<List<SuggestHitUi>>): String =
    when (state) {
        Resource.Idle, Resource.Loading -> "suggest gaps"
        is Resource.Error -> "unavailable"
        is Resource.Ready -> "${state.value.size} gaps surfaced"
    }

internal fun sourcesDetail(state: Resource<List<SourceEntryUi>>): String =
    when (state) {
        Resource.Idle, Resource.Loading -> "loading"
        is Resource.Error -> "unavailable"
        is Resource.Ready -> "${state.value.size} docs"
    }

internal fun domainsDetail(state: Resource<List<DomainFacetUi>>): String =
    when (state) {
        Resource.Idle, Resource.Loading -> "loading"
        is Resource.Error -> "unavailable"
        is Resource.Ready -> "${state.value.size} domains"
    }

internal fun statsDetail(state: Resource<JsonElement>): String =
    when (state) {
        Resource.Idle, Resource.Loading -> "loading"
        is Resource.Error -> "unavailable"
        is Resource.Ready -> vectorCount(state.value)?.let { "${formatCount(it)} vectors" } ?: "stats loaded"
    }

private fun vectorCount(element: JsonElement): Long? {
    val candidates = mutableListOf<Long>()
    fun visit(node: JsonElement, key: String?) {
        when (node) {
            is JsonObject -> node.forEach { (childKey, child) -> visit(child, childKey) }
            is JsonArray -> node.forEach { visit(it, key) }
            is JsonPrimitive -> {
                if (key in VECTOR_KEYS) {
                    node.jsonPrimitive.contentOrNull?.toLongOrNull()?.let(candidates::add)
                }
            }
        }
    }
    visit(element, null)
    return candidates.maxOrNull()
}

private fun formatCount(value: Long): String =
    "%,d".format(value)

private val VECTOR_KEYS = setOf(
    "vectors",
    "vector_count",
    "vectors_count",
    "indexed_vectors",
    "indexed_vectors_count",
    "total_vectors",
    "points",
    "points_count",
)
