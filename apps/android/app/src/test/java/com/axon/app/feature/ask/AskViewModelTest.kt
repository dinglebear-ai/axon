package com.axon.app.feature.ask

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class FollowUpQueryBuilderTest {
    @Test fun `no prior turns returns the question unchanged`() {
        val out = buildFollowUpQuery(prior = emptyList(), question = "what is rust?")
        assertEquals("what is rust?", out)
    }

    @Test fun `prior turns are rendered as Q-A pairs followed by the new question`() {
        val out =
            buildFollowUpQuery(
                prior = listOf(AskTurn("intro?", "intro answer."), AskTurn("more?", "more answer.")),
                question = "third?",
            )
        val expected =
            """
            Q: intro?
            A: intro answer.

            Q: more?
            A: more answer.

            third?
            """.trimIndent()
        assertEquals(expected, out)
    }

    @Test fun `turns window caps at six (oldest dropped)`() {
        val many = (1..8).map { AskTurn("q$it", "a$it") }
        val out = buildFollowUpQuery(prior = many, question = "final?")
        assertTrue("expected q3 onward, got: $out", out.startsWith("Q: q3\nA: a3"))
        assertTrue(!out.contains("Q: q1"))
        assertTrue(!out.contains("Q: q2"))
    }

    @Test fun `operation context is included in next effective prompt with axon skill hint`() {
        val turn =
            AskTurn(
                operationContextQuestion("Index site"),
                operationContextAnswer(
                    opLabel = "Index site",
                    target = "https://example.com",
                    status = "Completed",
                    endpoint = "POST /v1/sources",
                    jobId = "job-123",
                    summary = "12 pages crawled",
                    detail = "Site indexing completed from mobile.",
                ),
            )
        val out = buildFollowUpQuery(prior = listOf(turn), question = "what did it find?")

        assertTrue(out.contains("Q: Axon mobile operation: Index site"))
        assertTrue(out.contains("Target: https://example.com"))
        assertTrue(out.contains("Job ID: job-123"))
        assertTrue(out.contains("load the axon or axon:using-axon skill"))
        assertTrue(out.endsWith("what did it find?"))
    }
}

class RegenerateAfterStopTest {
    @Test fun `partial-then-stopped ask does not evict the previous good turn on regenerate`() {
        val state = AskRegenerationState()
        val turns = listOf(AskTurn("what is rust?", "Rust is a systems language."))
        state.started()
        assertEquals(
            "previous good turn must survive regenerate after a partial-then-stop",
            turns,
            state.turnsBeforeRegeneration(turns),
        )
    }

    @Test fun `regenerate after a completed answer drops only that turn`() {
        val state = AskRegenerationState()
        state.completed()
        assertEquals(
            listOf(AskTurn("q1", "a1")),
            state.turnsBeforeRegeneration(listOf(AskTurn("q1", "a1"), AskTurn("q2", "a2"))),
        )
    }
}
