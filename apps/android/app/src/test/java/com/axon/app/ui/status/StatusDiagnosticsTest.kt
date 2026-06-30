package com.axon.app.ui.status

import org.junit.Assert.assertEquals
import org.junit.Test

class StatusDiagnosticsTest {
    @Test
    fun `redactUrlUserInfo removes embedded credentials`() {
        assertEquals(
            "https://axon.example.test:8443/health",
            redactUrlUserInfo("https://user:secret@axon.example.test:8443/health?verbose=true#top"),
        )
    }

    @Test
    fun `redactUrlUserInfo removes query and fragment from ordinary URLs`() {
        assertEquals(
            "https://axon.example.test:8443/health",
            redactUrlUserInfo("https://axon.example.test:8443/health?token=secret#top"),
        )
    }

    @Test
    fun `redactUrlUserInfo preserves ordinary URLs`() {
        assertEquals(
            "https://axon.example.test",
            redactUrlUserInfo("https://axon.example.test"),
        )
    }

    @Test
    fun `healthCheckOriginUrl strips path query fragment and userinfo`() {
        assertEquals(
            "https://axon.example.test:8443",
            healthCheckOriginUrl("https://user:secret@axon.example.test:8443/$(touch pwn)?token=secret#top"),
        )
    }
}
