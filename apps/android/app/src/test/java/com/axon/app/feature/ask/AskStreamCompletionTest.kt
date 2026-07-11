package com.axon.app.feature.ask

import org.junit.Assert.assertEquals
import org.junit.Test

class AskStreamCompletionTest {
    @Test
    fun `done answer wins when present`() {
        assertEquals(
            "final answer",
            resolvedDoneAnswer(doneAnswer = "final answer", accumulatedAnswer = "streamed answer"),
        )
    }

    @Test
    fun `blank done answer preserves streamed deltas`() {
        assertEquals(
            "streamed answer",
            resolvedDoneAnswer(doneAnswer = "", accumulatedAnswer = "streamed answer"),
        )
    }
}
