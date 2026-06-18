package com.axon.app.ui.ask

import com.axon.app.ui.ingest.IngestSource
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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

    @Test fun `operation context is included in next effective prompt with axon skill hint`() {
        val turn = AskTurn(
            operationContextQuestion("Crawl"),
            operationContextAnswer(
                opLabel = "Crawl",
                target = "https://example.com",
                status = "Completed",
                endpoint = "POST /v1/crawl",
                jobId = "job-123",
                summary = "12 pages crawled",
                detail = "Crawl completed from mobile.",
            ),
        )
        val out = buildFollowUpQuery(prior = listOf(turn), question = "what did it find?")

        assertTrue(out.contains("Q: Axon mobile operation: Crawl"))
        assertTrue(out.contains("Target: https://example.com"))
        assertTrue(out.contains("Job ID: job-123"))
        assertTrue(out.contains("load the axon or axon:using-axon skill"))
        assertTrue(out.endsWith("what did it find?"))
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

/**
 * Regression guard for "regenerate after a partial-then-stopped answer".
 *
 * The real [AskViewModel] is an [androidx.lifecycle.AndroidViewModel] whose
 * `container` ([com.axon.app.di.AppContainer]) is a `lateinit … private set`
 * that builds Room/network/DataStore deps in [com.axon.app.AxonApp.onCreate],
 * so it can't be instantiated in a plain unit test without refactoring
 * production. Per the same trade-off documented in
 * [com.axon.app.ui.summarize.SummarizeViewModelTest], we exercise a stand-in
 * that mirrors the production turns + stop + regenerate state machine.
 *
 * Behavior under test: a turn is "stored" iff [AskViewModel.appendTurn] ran,
 * tracked by the `lastAskProducedTurn` flag — NOT inferred from the frozen
 * answer text. A partial-then-stopped answer keeps its streamed text, so the
 * old text-sniffing heuristic wrongly concluded a turn was stored and evicted
 * the PREVIOUS good turn. If production's flag wiring changes, update this too.
 */
class RegenerateAfterStopTest {
    @Test fun `partial-then-stopped ask does not evict the previous good turn on regenerate`() {
        val vm = AskTurnRegenStandIn()

        // Turn 1 completes normally → stored as a good turn.
        vm.askComplete(query = "what is rust?", answer = "Rust is a systems language.")
        assertEquals(listOf(AskTurn("what is rust?", "Rust is a systems language.")), vm.turns)

        // Turn 2 streams partial text, then the user stops. stopGeneration() never
        // calls appendTurn, so no turn is stored and the flag stays false.
        vm.askThenStop(query = "tell me more", partial = "The answer is")
        assertEquals("stop must not append a turn", 1, vm.turns.size)
        assertFalse("a stopped ask did not produce a turn", vm.producedTurnForTest)

        // Regenerate the stopped turn → must NOT drop turn 1.
        vm.regenerateLast()
        assertEquals(
            "previous good turn must survive regenerate after a partial-then-stop",
            listOf(AskTurn("what is rust?", "Rust is a systems language.")),
            vm.turns,
        )
    }

    @Test fun `regenerate after a completed answer drops only that turn`() {
        val vm = AskTurnRegenStandIn()
        vm.askComplete("q1", "a1")
        vm.askComplete("q2", "a2")
        assertEquals(2, vm.turns.size)

        vm.regenerateLast() // regenerating q2's completed answer
        assertEquals(listOf(AskTurn("q1", "a1")), vm.turns)
    }

    @Test fun `the old text-sniffing heuristic would have wrongly dropped the turn`() {
        // Pre-fix logic inferred "a turn was stored" from the frozen answer text:
        //   producedTurn = !text.startsWith("Error:") && text != "Stopped."
        // A partial-then-stopped answer keeps its streamed text, satisfying that
        // heuristic — the bug. The flag-based signal is correctly false instead.
        val frozenStoppedText = "The answer is"
        val oldHeuristicWouldDrop =
            !frozenStoppedText.startsWith("Error:") && frozenStoppedText != "Stopped."
        assertTrue("documents the heuristic the flag replaces", oldHeuristicWouldDrop)

        val vm = AskTurnRegenStandIn()
        vm.askThenStop(query = "q", partial = frozenStoppedText)
        assertFalse("flag is the correct signal — no turn produced", vm.producedTurnForTest)
    }
}

/**
 * Near-copy of [AskViewModel]'s turns/stop/regenerate slice (AndroidViewModel
 * plumbing and streaming I/O removed). Mirrors:
 *  - ask(): resets the produced-turn flag, appends a UserMsg + blank streaming
 *    AxonMsg; on a Done event appends a turn and sets the flag true.
 *  - stopGeneration(): freezes the partial AxonMsg, appends NO turn, leaves the
 *    flag false.
 *  - regenerateLast(): drops the last stored turn ONLY when the flag is set.
 */
private class AskTurnRegenStandIn {
    val turns = mutableListOf<AskTurn>()
    private val chatItems = mutableListOf<ChatItem>()
    private var lastAskProducedTurn = false

    val producedTurnForTest: Boolean get() = lastAskProducedTurn

    private fun startAsk(query: String) {
        lastAskProducedTurn = false
        chatItems.add(ChatItem.UserMsg(query))
        chatItems.add(ChatItem.AxonMsg(text = "", isStreaming = true))
    }

    private fun replaceLastAxon(text: String, streaming: Boolean) {
        val idx = chatItems.indexOfLast { it is ChatItem.AxonMsg }
        if (idx >= 0) {
            chatItems[idx] = (chatItems[idx] as ChatItem.AxonMsg).copy(text = text, isStreaming = streaming)
        }
    }

    /** ask() that streams then receives a Done event with the full answer. */
    fun askComplete(query: String, answer: String) {
        startAsk(query)
        replaceLastAxon(answer, streaming = true) // a delta flush
        replaceLastAxon(answer, streaming = false) // Done
        appendTurn(query, answer)
        lastAskProducedTurn = true
    }

    /** ask() that streams partial text, then the user invokes stopGeneration(). */
    fun askThenStop(query: String, partial: String) {
        startAsk(query)
        replaceLastAxon(partial, streaming = true)
        stopGeneration()
    }

    private fun appendTurn(q: String, a: String) {
        val next = (turns + AskTurn(q, a.take(500))).takeLast(MAX_FOLLOW_UP_TURNS)
        turns.clear()
        turns.addAll(next)
    }

    private fun stopGeneration() {
        val idx = chatItems.indexOfLast { it is ChatItem.AxonMsg }
        if (idx >= 0) {
            val msg = chatItems[idx] as ChatItem.AxonMsg
            chatItems[idx] = msg.copy(text = msg.text.ifBlank { "Stopped." }, isStreaming = false)
        }
        // Deliberately NO appendTurn and NO flag flip — matches production.
    }

    fun regenerateLast() {
        val userIdx = chatItems.indexOfLast { it is ChatItem.UserMsg }
        if (userIdx < 0) return
        if (lastAskProducedTurn && turns.isNotEmpty()) {
            turns.removeAt(turns.lastIndex)
        }
        // Truncate chat back to before the regenerated user message (production
        // then re-asks, re-appending the question — not modeled here).
        while (chatItems.size > userIdx) chatItems.removeAt(chatItems.lastIndex)
    }
}
