package com.axon.app.ui.fab

import org.junit.Assert.assertEquals
import org.junit.Test

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
}
