package com.axon.app.ui.settings

import app.cash.turbine.test
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
 * Tests for [SettingsViewModel] state contract via stand-ins that mirror the
 * production state machines (save and test-connection flows) without requiring
 * Robolectric or the [com.axon.app.AxonApp] container.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SettingsViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() { Dispatchers.setMain(dispatcher) }
    @After fun tearDown() { Dispatchers.resetMain() }

    // ── SaveSettings ──────────────────────────────────────────────────────────

    @Test fun `saveSettings success transitions Idle → Saving → Saved`() = runTest(dispatcher) {
        val vm = TestSettingsViewModel(saveResult = Result.success(Unit))
        vm.saveState.test {
            assertEquals(SaveState.Idle, awaitItem())
            vm.save("https://axon.example.com", "tok", "axon")
            assertEquals(SaveState.Saving, awaitItem())
            assertEquals(SaveState.Saved, awaitItem())
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `saveSettings failure transitions Idle → Saving → Failed with message`() = runTest(dispatcher) {
        val vm = TestSettingsViewModel(saveResult = Result.failure(RuntimeException("disk full")))
        vm.saveState.test {
            assertEquals(SaveState.Idle, awaitItem())
            vm.save("https://axon.example.com", "tok", "axon")
            assertEquals(SaveState.Saving, awaitItem())
            val failed = awaitItem() as SaveState.Failed
            assertTrue(failed.error.contains("disk full"))
            cancelAndIgnoreRemainingEvents()
        }
    }

    // ── TestConnection ────────────────────────────────────────────────────────

    @Test fun `testConnection success transitions Idle → Testing → Ok`() = runTest(dispatcher) {
        val vm = TestSettingsViewModel(pingResult = Result.success(Unit))
        vm.connection.test {
            assertEquals(TestConnectionState.Idle, awaitItem())
            vm.testConnection("https://axon.example.com", "tok")
            assertEquals(TestConnectionState.Testing, awaitItem())
            val ok = awaitItem() as TestConnectionState.Ok
            assertNull("no cleartext warning for https", ok.warning)
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `testConnection with http URL shows cleartext warning`() = runTest(dispatcher) {
        val vm = TestSettingsViewModel(pingResult = Result.success(Unit))
        vm.testConnection("http://axon.example.com", "tok")
        val ok = vm.connection.value as TestConnectionState.Ok
        assertNotNull("cleartext http must produce a warning", ok.warning)
    }

    @Test fun `testConnection failure transitions to Failed with error message`() = runTest(dispatcher) {
        val vm = TestSettingsViewModel(pingResult = Result.failure(RuntimeException("connection refused")))
        vm.connection.test {
            assertEquals(TestConnectionState.Idle, awaitItem())
            vm.testConnection("https://axon.example.com", "tok")
            assertEquals(TestConnectionState.Testing, awaitItem())
            val failed = awaitItem() as TestConnectionState.Failed
            assertTrue(failed.error.contains("connection refused"))
            cancelAndIgnoreRemainingEvents()
        }
    }
}

private class TestSettingsViewModel(
    private val saveResult: Result<Unit> = Result.success(Unit),
    private val pingResult: Result<Unit> = Result.success(Unit),
) {
    private val _saveState = MutableStateFlow<SaveState>(SaveState.Idle)
    val saveState = _saveState.asStateFlow()

    private val _connection = MutableStateFlow<TestConnectionState>(TestConnectionState.Idle)
    val connection = _connection.asStateFlow()

    fun save(serverUrl: String, @Suppress("UNUSED_PARAMETER") token: String, @Suppress("UNUSED_PARAMETER") collection: String) {
        _saveState.value = SaveState.Saving
        saveResult.fold(
            onSuccess = { _saveState.value = SaveState.Saved },
            onFailure = { _saveState.value = SaveState.Failed(it.message ?: "Failed to save settings") },
        )
    }

    fun testConnection(serverUrl: String, @Suppress("UNUSED_PARAMETER") token: String) {
        _connection.value = TestConnectionState.Testing
        pingResult.fold(
            onSuccess = {
                val warning = if (serverUrl.trim().startsWith("http://")) {
                    "Warning: cleartext HTTP is in use. Consider switching to HTTPS for non-Tailscale servers."
                } else null
                _connection.value = TestConnectionState.Ok(warning = warning)
            },
            onFailure = { _connection.value = TestConnectionState.Failed(it.message ?: "Server unreachable") },
        )
    }
}
