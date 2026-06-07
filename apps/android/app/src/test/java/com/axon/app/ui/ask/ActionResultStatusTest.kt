package com.axon.app.ui.ask

import org.junit.Assert.assertEquals
import org.junit.Test

class ActionResultStatusTest {
    @Test
    fun `status classifier separates success queued and error states`() {
        assertEquals(ResultStatusKind.Success, resultStatusKind("200 OK"))
        assertEquals(ResultStatusKind.Warning, resultStatusKind("202 Accepted"))
        assertEquals(ResultStatusKind.Warning, resultStatusKind("running"))
        assertEquals(ResultStatusKind.Error, resultStatusKind("500 Internal Server Error"))
        assertEquals(ResultStatusKind.Error, resultStatusKind("failed"))
    }
}
