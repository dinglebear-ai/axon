package com.axon.app.ui.status

import app.cash.turbine.test
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.atomic.AtomicInteger

/**
 * Unit tests for the pure connection-state engine.
 *
 * Uses runTest's `backgroundScope` so the engine's never-ending poll flow
 * (`while(true) { delay(...) }`) is auto-cancelled when the test body exits.
 * Using `this` directly would leak the coroutine and cause
 * `UncompletedCoroutinesError`.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ConnectionStatusEngineTest {

    @Test fun `ping success emits Online`() = runTest {
        val engine = ConnectionStatusEngine(ping = { true })
        engine.state(backgroundScope).test {
            assertEquals(ConnectionState.Checking, awaitItem())
            assertEquals(ConnectionState.Online, awaitItem())
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `ping false emits Offline`() = runTest {
        val engine = ConnectionStatusEngine(ping = { false })
        engine.state(backgroundScope).test {
            assertEquals(ConnectionState.Checking, awaitItem())
            assertEquals(ConnectionState.Offline, awaitItem())
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `ping throw is caught and emits Offline`() = runTest {
        val engine = ConnectionStatusEngine(
            ping = { throw RuntimeException("network unreachable") },
        )
        engine.state(backgroundScope).test {
            assertEquals(ConnectionState.Checking, awaitItem())
            // The .catch operator converts the thrown exception into a single Offline
            // emission. Without it the flow would tear down with no terminal value.
            assertEquals(ConnectionState.Offline, awaitItem())
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `state transitions from Online to Offline as ping flips`() = runTest {
        val online = AtomicInteger(1)
        val engine = ConnectionStatusEngine(
            ping = { online.get() == 1 },
            pollIntervalMs = 1_000,
        )
        engine.state(backgroundScope).test {
            assertEquals(ConnectionState.Checking, awaitItem())
            assertEquals(ConnectionState.Online, awaitItem())
            online.set(0)
            advanceTimeBy(1_100)
            // The next poll tick should observe the flipped value.
            assertEquals(ConnectionState.Offline, awaitItem())
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `refresh triggers an out-of-band ping`() = runTest {
        val calls = AtomicInteger(0)
        val engine = ConnectionStatusEngine(
            ping = {
                calls.incrementAndGet()
                true
            },
            pollIntervalMs = 1_000_000, // effectively suppress the timed poll
        )
        engine.state(backgroundScope).test {
            assertEquals(ConnectionState.Checking, awaitItem())
            assertEquals(ConnectionState.Online, awaitItem())
            val before = calls.get()
            engine.refresh()
            // StateFlow deduplicates equal values so we don't await a new emission;
            // instead we assert that a fresh ping ran in response to the refresh.
            // Drain the virtual clock so the refresh-triggered branch runs to completion.
            advanceTimeBy(100)
            assertTrue("expected a fresh ping after refresh: before=$before, after=${calls.get()}", calls.get() > before)
            cancelAndIgnoreRemainingEvents()
        }
    }
}
