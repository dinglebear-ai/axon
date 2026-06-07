package com.axon.app.ui.ask

import com.axon.app.ui.ingest.IngestSource
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FollowUpQueryBuilderTest {
    @Test fun `no prior turns returns the question unchanged`() {
        val out = buildFollowUpQuery(prior = emptyList(), question = "what is rust?")
        assertEquals("what is rust?", out)
    }

    @Test fun `prior turns are rendered as Q-A pairs followed by the new question`() {
        val out = buildFollowUpQuery(
            prior = listOf(AskTurn("intro?", "intro answer."), AskTurn("more?", "more answer.")),
            question = "third?"
        )
        val expected = """
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
}

class FabIngestSourceInferenceTest {
    @Test fun `infers github from canonical host`() {
        val inferred = inferFabIngestSource("https://github.com/owner/repo")
        assertEquals(IngestSource.Github, inferred.getOrThrow())
    }

    @Test fun `infers github from valid subdomain`() {
        val inferred = inferFabIngestSource("https://api.github.com/repos/owner/repo")
        assertEquals(IngestSource.Github, inferred.getOrThrow())
    }

    @Test fun `rejects github lookalike host`() {
        val inferred = inferFabIngestSource("https://github.com.attacker.com/owner/repo")
        assertTrue(inferred.isFailure)
        assertNotNull(inferred.exceptionOrNull()?.message)
    }

    @Test fun `infers github shorthand without requiring URL syntax`() {
        val inferred = inferFabIngestSource("github/owner/repo")
        assertEquals(IngestSource.Github, inferred.getOrThrow())
    }

    @Test fun `infers reddit shorthand`() {
        val inferred = inferFabIngestSource("r/rust")
        assertEquals(IngestSource.Reddit, inferred.getOrThrow())
    }
}
