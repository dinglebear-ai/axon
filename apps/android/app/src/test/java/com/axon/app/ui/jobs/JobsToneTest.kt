package com.axon.app.ui.jobs

import org.junit.Assert.assertEquals
import org.junit.Test
import tv.tootie.aurora.components.AuroraStatusTone

/**
 * Direct test of [toneFor] — the function is `internal` so this test imports
 * and exercises it rather than maintaining a tautological replica (the anti-pattern
 * present in the v1 plan).
 */
class JobsToneTest {

    @Test fun `pending and queued map to Queued`() {
        assertEquals(AuroraStatusTone.Queued, toneFor("pending"))
        assertEquals(AuroraStatusTone.Queued, toneFor("queued"))
        assertEquals(AuroraStatusTone.Queued, toneFor("Queued")) // case-insensitive
    }

    @Test fun `running and in_progress map to Syncing`() {
        assertEquals(AuroraStatusTone.Syncing, toneFor("running"))
        assertEquals(AuroraStatusTone.Syncing, toneFor("in_progress"))
    }

    @Test fun `completed and succeeded map to Online`() {
        assertEquals(AuroraStatusTone.Online, toneFor("completed"))
        assertEquals(AuroraStatusTone.Online, toneFor("succeeded"))
    }

    @Test fun `failed and error map to Error`() {
        assertEquals(AuroraStatusTone.Error, toneFor("failed"))
        assertEquals(AuroraStatusTone.Error, toneFor("error"))
    }

    @Test fun `cancelled and canceled both map to Offline`() {
        assertEquals(AuroraStatusTone.Offline, toneFor("cancelled"))
        assertEquals(AuroraStatusTone.Offline, toneFor("canceled"))
    }

    @Test fun `unknown status falls back to Degraded`() {
        assertEquals(AuroraStatusTone.Degraded, toneFor("ufo-sighting"))
        assertEquals(AuroraStatusTone.Degraded, toneFor(""))
    }
}
