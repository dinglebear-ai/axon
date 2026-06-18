package com.axon.app.ui.ingest

import app.cash.turbine.test
import com.axon.app.data.repository.JobUi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * Tests for the Ingest VM's *contract* — R13 URL.host endsWith validation, and the submit /
 * checkStatus / cancel state-machine — via a stand-in (same pattern as Summarize/Knowledge VM tests).
 *
 * The R13 bypass test is the load-bearing one: `https://github.com.attacker.com/x` MUST be
 * rejected as a Github target. A plain `target.contains("github.com")` check (the v1 plan body)
 * would have let that through.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class IngestViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() { Dispatchers.setMain(dispatcher) }
    @After fun tearDown() { Dispatchers.resetMain() }

    // ── R13 validate() ─────────────────────────────────────────────────────────

    @Test fun `github accepts canonical github_com URL`() {
        assertNull(IngestSource.Github.validate("https://github.com/owner/repo"))
    }

    @Test fun `github accepts subdomain of github_com`() {
        assertNull(IngestSource.Github.validate("https://api.github.com/repos/x"))
    }

    @Test fun `github rejects lookalike host github_com_attacker_com (R13 bypass)`() {
        val err = IngestSource.Github.validate("https://github.com.attacker.com/owner/repo")
        assertNotNull("expected rejection, got null", err)
    }

    @Test fun `github rejects unrelated host`() {
        assertNotNull(IngestSource.Github.validate("https://example.com/owner/repo"))
    }

    @Test fun `github accepts non-URL form (ssh-style) and defers to server`() {
        assertNull(IngestSource.Github.validate("git@github.com:owner/repo.git"))
    }

    @Test fun `github is case-insensitive on host`() {
        assertNull(IngestSource.Github.validate("https://GitHub.com/owner/repo"))
    }

    @Test fun `gitea source has no host hint so accepts any URL`() {
        assertNull(IngestSource.Gitea.validate("https://my-gitea.example.com/owner/repo"))
    }

    @Test fun `blank target is always rejected`() {
        assertNotNull(IngestSource.Github.validate(""))
        assertNotNull(IngestSource.Gitea.validate("   "))
    }

    @Test fun `youtube accepts canonical youtube_com URL`() {
        assertNull(IngestSource.Youtube.validate("https://youtube.com/watch?v=abc"))
    }

    @Test fun `reddit rejects lookalike host`() {
        assertNotNull(IngestSource.Reddit.validate("https://reddit.com.evil.example/r/rust"))
    }

    // ── State machine ─────────────────────────────────────────────────────────

    @Test fun `submit success transitions Idle to Submitting to Submitted and records jobId`() = runTest(dispatcher) {
        val vm = TestIngestViewModel(submitResult = Result.success("job-123"))
        vm.uiState.test {
            assertEquals(IngestUi.Idle, awaitItem())
            vm.submit(IngestSource.Github, "https://github.com/owner/repo")
            assertEquals(IngestUi.Submitting, awaitItem())
            val submitted = awaitItem() as IngestUi.Submitted
            assertEquals("job-123", submitted.jobId)
            assertEquals(IngestSource.Github, submitted.source)
            assertEquals(1, vm.recordedJobIds.size)
            assertEquals("job-123", vm.recordedJobIds[0])
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `submit with invalid target emits Error and never calls repo`() = runTest(dispatcher) {
        val vm = TestIngestViewModel(submitResult = Result.success("never"))
        vm.submit(IngestSource.Github, "https://github.com.attacker.com/x")
        val s = vm.uiState.value
        assertTrue("expected Error, got $s", s is IngestUi.Error)
        assertEquals(0, vm.submitCalls)
    }

    @Test fun `submit repo failure emits Error`() = runTest(dispatcher) {
        val vm = TestIngestViewModel(submitResult = Result.failure(IllegalStateException("boom")))
        vm.uiState.test {
            assertEquals(IngestUi.Idle, awaitItem())
            vm.submit(IngestSource.Github, "https://github.com/owner/repo")
            assertEquals(IngestUi.Submitting, awaitItem())
            val err = awaitItem() as IngestUi.Error
            assertTrue(err.message.contains("boom"))
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `checkStatus emits Status with returned job`() = runTest(dispatcher) {
        val job = JobUi(
            id = "job-1", status = "completed", url = null, sourceType = "github",
            target = "https://github.com/x", errorText = null, resultJson = null,
        )
        val vm = TestIngestViewModel(
            submitResult = Result.success("job-1"),
            statusResult = Result.success(job),
        )
        vm.checkStatus("job-1")
        dispatcher.scheduler.advanceUntilIdle()
        val s = vm.uiState.value as IngestUi.Status
        assertEquals("completed", s.job.status)
    }

    @Test fun `cancel calls repo cancel then checkStatus and emits Status`() = runTest(dispatcher) {
        val job = JobUi(
            id = "job-1", status = "canceled", url = null, sourceType = "github",
            target = "https://github.com/x", errorText = null, resultJson = null,
        )
        val vm = TestIngestViewModel(
            submitResult = Result.success("job-1"),
            statusResult = Result.success(job),
        )
        vm.cancel("job-1")
        dispatcher.scheduler.advanceUntilIdle()
        assertEquals(1, vm.cancelCalls)
        assertEquals(1, vm.statusCalls)
        val s = vm.uiState.value as IngestUi.Status
        assertEquals("canceled", s.job.status)
    }

    @Test fun `reset returns to Idle from any state`() = runTest(dispatcher) {
        val vm = TestIngestViewModel(submitResult = Result.failure(IllegalStateException("x")))
        vm.submit(IngestSource.Github, "https://github.com/x")
        dispatcher.scheduler.advanceUntilIdle()
        assertTrue(vm.uiState.value is IngestUi.Error)
        vm.reset()
        assertEquals(IngestUi.Idle, vm.uiState.value)
    }
}

/**
 * Mirrors the production [IngestViewModel] state contract without AndroidViewModel plumbing.
 * Counts recorded jobIds, submit/cancel/status calls so tests can assert side effects.
 */
private class TestIngestViewModel(
    private val submitResult: Result<String>,
    private val statusResult: Result<JobUi> = Result.failure(IllegalStateException("no status stub")),
) {
    var submitCalls: Int = 0
    var cancelCalls: Int = 0
    var statusCalls: Int = 0
    val recordedJobIds: MutableList<String> = mutableListOf()

    private val _uiState = MutableStateFlow<IngestUi>(IngestUi.Idle)
    val uiState = _uiState.asStateFlow()

    fun submit(source: IngestSource, target: String) {
        source.validate(target)?.let { msg ->
            _uiState.value = IngestUi.Error(msg)
            return
        }
        submitCalls++
        _uiState.value = IngestUi.Submitting
        submitResult.fold(
            onSuccess = { jobId ->
                recordedJobIds += jobId
                _uiState.value = IngestUi.Submitted(jobId, source, target)
            },
            onFailure = { _uiState.value = IngestUi.Error(it.message ?: "Error") },
        )
    }

    fun checkStatus(@Suppress("UNUSED_PARAMETER") jobId: String) {
        statusCalls++
        statusResult.fold(
            onSuccess = { _uiState.value = IngestUi.Status(it) },
            onFailure = { _uiState.value = IngestUi.Error(it.message ?: "Error") },
        )
    }

    fun cancel(jobId: String) {
        cancelCalls++
        checkStatus(jobId)
    }

    fun reset() { _uiState.value = IngestUi.Idle }
}
