package com.axon.app.ui.fab

import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Path

class FabOpInputCardTest {
    @Test
    fun urlOperationsAddHttpsSchemeForBareDomains() {
        assertEquals("https://example.com", normalizeFabInput(FabOp.Scrape, "example.com"))
        assertEquals("https://docs.rs", normalizeFabInput(FabOp.Crawl, " docs.rs "))
    }

    @Test
    fun urlOperationsPreserveExplicitSchemes() {
        assertEquals("http://example.com", normalizeFabInput(FabOp.Map, "http://example.com"))
        assertEquals("https://example.com/docs", normalizeFabInput(FabOp.Retrieve, "https://example.com/docs"))
    }

    @Test
    fun queryOperationsDoNotRewriteText() {
        assertEquals("example.com", normalizeFabInput(FabOp.Search, "example.com"))
        assertEquals("axon jobs", normalizeFabInput(FabOp.Query, " axon jobs "))
    }

    @Test
    fun broadFabActionsRequireExplicitConfirmation() {
        assertEquals(false, fabInputCanSubmit(FabOp.Crawl, "https://example.com", broadActionConfirmed = false))
        assertEquals(false, fabInputCanSubmit(FabOp.Ingest, "github/owner/repo", broadActionConfirmed = false))

        assertEquals(true, fabInputCanSubmit(FabOp.Crawl, "https://example.com", broadActionConfirmed = true))
        assertEquals(true, fabInputCanSubmit(FabOp.Ingest, "github/owner/repo", broadActionConfirmed = true))
        assertEquals(true, fabInputCanSubmit(FabOp.Scrape, "https://example.com", broadActionConfirmed = false))
    }

    @Test
    fun broadFabConfirmationLabelsNameCurrentOptions() {
        assertEquals("Run with current crawl defaults/options", FabOp.Crawl.broadActionConfirmationLabel())
        assertEquals("Run with current ingest defaults/options", FabOp.Ingest.broadActionConfirmationLabel())
        assertEquals(null, FabOp.Scrape.broadActionConfirmationLabel())
    }

    @Test
    fun fabInputBindsImeSendToSubmitHandler() {
        val sourcePath = listOf(
            Path.of("src/main/java/com/axon/app/ui/fab/FabOpInputCard.kt"),
            Path.of("app/src/main/java/com/axon/app/ui/fab/FabOpInputCard.kt"),
            Path.of("apps/android/app/src/main/java/com/axon/app/ui/fab/FabOpInputCard.kt"),
        ).first { it.toFile().isFile }
        val source = sourcePath.toFile().readText()

        assert(source.contains("KeyboardActions(onSend = {") && source.contains("submitIfReady()")) {
            "FabOpInputCard must wire IME Send to the same submit path as the send button"
        }
    }
}
