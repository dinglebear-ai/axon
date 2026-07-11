package com.axon.app.feature.settings

import app.cash.turbine.test
import com.axon.app.core.auth.AuthMode
import com.axon.app.core.auth.authModeFromWireValue
import com.axon.app.data.repository.AxonSettings
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
import org.junit.Assert.assertFalse
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
        vm.testConnection("http://dookie.manatee-triceratops.ts.net:8001", "tok")
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

    @Test
    fun `setDraftAuthMode does not persist oauth before successful sign in`() = runTest(dispatcher) {
        val vm = TestSettingsViewModel(initialPersistedAuthMode = AuthMode.Bearer)

        vm.setDraftAuthMode(AuthMode.OAuth)

        assertEquals(AuthMode.OAuth, vm.draftAuthMode.value)
        assertEquals(AuthMode.Bearer, vm.persistedAuthMode.value)
    }

    @Test
    fun `cancelOAuthSignIn shows failure and keeps oauth-first draft mode`() = runTest(dispatcher) {
        val vm = TestSettingsViewModel()

        vm.cancelOAuthSignIn()

        assertTrue(vm.saveState.value is SaveState.Failed)
        assertEquals(AuthMode.OAuth, vm.draftAuthMode.value)
    }

    @Test
    fun `signOutOAuth does not switch mode when encrypted clear fails`() = runTest(dispatcher) {
        val vm = TestSettingsViewModel(oauthClearResult = false)
        vm.setDraftAuthMode(AuthMode.OAuth)

        vm.signOutOAuth()

        assertTrue(vm.saveState.value is SaveState.Failed)
        assertEquals(AuthMode.OAuth, vm.draftAuthMode.value)
        assertEquals(AuthMode.OAuth, vm.persistedAuthMode.value)
    }
}

class SettingsSecurityHelpersTest {
    @Test fun `new settings default to oauth auth mode`() {
        assertEquals(AuthMode.OAuth, AxonSettings().authMode)
    }

    @Test fun `missing persisted auth mode decodes to oauth for first run`() {
        assertEquals(AuthMode.OAuth, authModeFromWireValue(null))
        assertEquals(AuthMode.OAuth, authModeFromWireValue(""))
        assertEquals(AuthMode.Bearer, authModeFromWireValue("bearer"))
    }

    @Test fun `missing persisted auth mode preserves upgraded bearer installs`() {
        assertEquals(AuthMode.Bearer, authModeFromWireValue(null, hasBearerToken = true))
        assertEquals(AuthMode.Bearer, authModeFromWireValue("", hasBearerToken = true))
        assertEquals(AuthMode.OAuth, authModeFromWireValue(null, hasBearerToken = false))
    }

    @Test fun `validateServerUrl rejects cleartext outside tailnet allowlist`() {
        val result = runCatching { validateAxonServerUrl("http://axon.example.com") }
        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull()?.message.orEmpty().contains("HTTPS"))
    }

    @Test fun `validateServerUrl accepts cleartext for configured tailscale domains`() {
        validateAxonServerUrl("http://dookie.manatee-triceratops.ts.net:8001")
        validateAxonServerUrl("http://dookie.manatee-triceratops.tailvpn.net:8001")
    }

    @Test fun `redacts explicit env secrets from values loaded into UI`() {
        val explicit = mapOf(
            "GITHUB_TOKEN" to "ghp_should_not_be_in_state",
            "QDRANT_URL" to "http://qdrant:6333",
        )

        val redacted = redactConfigValuesForUi(explicit, AxonSettingsCatalog.envSecretKeys)

        assertEquals(REDACTED_SECRET_VALUE, redacted["GITHUB_TOKEN"])
        assertEquals("http://qdrant:6333", redacted["QDRANT_URL"])
        assertFalse(redacted.values.any { it.contains("should_not_be_in_state") })
    }

    @Test fun `redacts raw env text before exposing it to UI state`() {
        val raw = """
            GITHUB_TOKEN=ghp_should_not_be_in_raw_state
            QDRANT_URL=http://qdrant:6333
        """.trimIndent()

        val redacted = redactEnvText(raw, AxonSettingsCatalog.envSecretKeys)

        assertTrue(redacted.contains("GITHUB_TOKEN=$REDACTED_SECRET_VALUE"))
        assertTrue(redacted.contains("QDRANT_URL=http://qdrant:6333"))
        assertFalse(redacted.contains("should_not_be_in_raw_state"))
    }

    @Test fun `redacted secret placeholders are not dirty save candidates`() {
        val values = mapOf(
            "GITHUB_TOKEN" to REDACTED_SECRET_VALUE,
            "QDRANT_URL" to "http://qdrant:6333",
        )

        val dirty = dirtyKeysForSecretSafeSave(
            values = values,
            dirtyKeys = setOf("GITHUB_TOKEN", "QDRANT_URL"),
            secretKeys = AxonSettingsCatalog.envSecretKeys,
        )

        assertEquals(setOf("QDRANT_URL"), dirty)
    }

    @Test fun `changed secret values remain dirty save candidates`() {
        val values = mapOf("GITHUB_TOKEN" to "ghp_replacement")

        val dirty = dirtyKeysForSecretSafeSave(
            values = values,
            dirtyKeys = setOf("GITHUB_TOKEN"),
            secretKeys = AxonSettingsCatalog.envSecretKeys,
        )

        assertEquals(setOf("GITHUB_TOKEN"), dirty)
    }
}

private class TestSettingsViewModel(
    private val saveResult: Result<Unit> = Result.success(Unit),
    private val pingResult: Result<Unit> = Result.success(Unit),
    private val oauthClearResult: Boolean = true,
    initialPersistedAuthMode: AuthMode = AuthMode.OAuth,
) {
    private val _saveState = MutableStateFlow<SaveState>(SaveState.Idle)
    val saveState = _saveState.asStateFlow()

    private val _connection = MutableStateFlow<TestConnectionState>(TestConnectionState.Idle)
    val connection = _connection.asStateFlow()

    private val _draftAuthMode = MutableStateFlow(initialPersistedAuthMode)
    val draftAuthMode = _draftAuthMode.asStateFlow()

    private val _persistedAuthMode = MutableStateFlow(initialPersistedAuthMode)
    val persistedAuthMode = _persistedAuthMode.asStateFlow()

    fun setDraftAuthMode(mode: AuthMode) {
        _draftAuthMode.value = mode
    }

    fun cancelOAuthSignIn() {
        _saveState.value = SaveState.Failed("OAuth sign-in was cancelled")
    }

    fun signOutOAuth() {
        if (!oauthClearResult) {
            _saveState.value = SaveState.Failed("Could not clear OAuth credentials")
            return
        }
        _persistedAuthMode.value = AuthMode.Bearer
        _draftAuthMode.value = AuthMode.Bearer
        _saveState.value = SaveState.Saved
    }

    fun save(serverUrl: String, @Suppress("UNUSED_PARAMETER") token: String, @Suppress("UNUSED_PARAMETER") collection: String) {
        _saveState.value = SaveState.Saving
        saveResult.fold(
            onSuccess = { _saveState.value = SaveState.Saved },
            onFailure = { _saveState.value = SaveState.Failed(it.message ?: "Failed to save settings") },
        )
    }

    fun testConnection(serverUrl: String, @Suppress("UNUSED_PARAMETER") token: String) {
        _connection.value = TestConnectionState.Testing
        runCatching { validateAxonServerUrl(serverUrl.trim()) }.fold(
            onSuccess = {
                pingResult.fold(
                    onSuccess = {
                        val warning = if (serverUrl.trim().startsWith("http://")) {
                            "Warning: cleartext HTTP is in use. Tailscale domains are allowed by Android network security."
                        } else null
                        _connection.value = TestConnectionState.Ok(warning = warning)
                    },
                    onFailure = { _connection.value = TestConnectionState.Failed(it.message ?: "Server unreachable") },
                )
            },
            onFailure = { _connection.value = TestConnectionState.Failed(it.message ?: "Invalid server URL") },
        )
    }
}
