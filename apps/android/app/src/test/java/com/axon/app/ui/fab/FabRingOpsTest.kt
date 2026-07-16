package com.axon.app.ui.fab

import org.junit.Assert.assertEquals
import org.junit.Test

class FabRingOpsTest {
    @Test
    fun ringOrderMatchesAndroidReference() {
        assertEquals(
            listOf(
                FabOp.Scrape,
                FabOp.Research,
                FabOp.Extract,
                FabOp.Query,
                FabOp.Search,
                FabOp.Map,
                FabOp.Retrieve,
                FabOp.Summarize,
                FabOp.SourceSite,
                FabOp.Source,
            ),
            FabRingOps,
        )
    }
}
