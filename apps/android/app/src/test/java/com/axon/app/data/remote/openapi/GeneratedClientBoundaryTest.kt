package com.axon.app.data.remote.openapi

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test

class GeneratedClientBoundaryTest {
    @Test
    fun generatedModelsAreOnlyImportedByGeneratedAdapter() {
        val root = File("src")
        val offenders = root.walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .filterNot { it.invariantSeparatorsPath.endsWith("/data/remote/GeneratedAxonApi.kt") }
            .filterNot { it.invariantSeparatorsPath.endsWith("/data/remote/openapi/GeneratedClientBoundaryTest.kt") }
            .filterNot { "/build/generated/" in it.invariantSeparatorsPath }
            .flatMap { file ->
                file.readLines()
                    .mapIndexedNotNull { index, line ->
                        if (line.contains("com.axon.app.generated.")) {
                            "${file.invariantSeparatorsPath}:${index + 1}: ${line.trim()}"
                        } else {
                            null
                        }
                    }
            }
            .toList()

        assertEquals(emptyList<String>(), offenders)
    }
}
