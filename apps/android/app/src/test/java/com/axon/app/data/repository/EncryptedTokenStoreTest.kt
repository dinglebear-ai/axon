package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assume.assumeNoException
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Tests for [EncryptedTokenStore] and the idempotent migration helper in
 * [SettingsRepository.migrateTokenToEncrypted].
 *
 * EncryptedSharedPreferences relies on AndroidKeyStore. Robolectric SDK 33+ ships
 * a working KeyStore shim, but we still call [assumeNoException] in [Before] so a
 * keystore failure on some CI image is *skipped* rather than red — the migration
 * logic is the contract under test, not the keystore HAL.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [33])
class EncryptedTokenStoreTest {
    private val ctx: Context = ApplicationProvider.getApplicationContext()
    private lateinit var store: EncryptedTokenStore

    @Before fun setUp() {
        store = EncryptedTokenStore(ctx)
        // Probe: write a sentinel with one store, then read it back with a *fresh*
        // store. The @Volatile cache inside a single store would mask a keystore
        // failure (write() caches even when prefs == null). Cross-instance round-trip
        // exercises the real backing file.
        store.write("__probe__")
        val readBack = EncryptedTokenStore(ctx).read()
        org.junit.Assume.assumeTrue(
            "Robolectric keystore unavailable — EncryptedSharedPreferences round-trip failed",
            readBack == "__probe__",
        )
        store.clear()
    }

    @After fun tearDown() = runBlocking {
        store.clear()
        ctx.settingsDataStore.edit { it.remove(LEGACY_KEY_TOKEN) }
        Unit
    }

    @Test fun `write then read round-trips token`() {
        store.write("hello-token")
        assertEquals("hello-token", store.read())
    }

    @Test fun `clear removes token and read returns null`() {
        store.write("temp")
        store.clear()
        assertNull(store.read())
    }

    @Test fun `migration moves legacy plaintext token into encrypted store`() = runBlocking {
        ctx.settingsDataStore.edit { it[LEGACY_KEY_TOKEN] = "legacy-secret" }
        store.clear()
        val repo = SettingsRepository(ctx, store)

        repo.migrateTokenToEncrypted()

        assertEquals("legacy-secret", store.read())
        assertNull(ctx.settingsDataStore.data.first()[LEGACY_KEY_TOKEN])
    }

    @Test fun `migration is idempotent — second call leaves encrypted token intact`() = runBlocking {
        ctx.settingsDataStore.edit { it[LEGACY_KEY_TOKEN] = "first-time" }
        store.clear()
        val repo = SettingsRepository(ctx, store)

        repo.migrateTokenToEncrypted()
        // Even if a stale plaintext entry sneaks back in, a second migration call
        // must NOT overwrite the value already in the encrypted store.
        ctx.settingsDataStore.edit { it[LEGACY_KEY_TOKEN] = "stale-second-write" }
        repo.migrateTokenToEncrypted()

        assertEquals("first-time", store.read())
        assertNull(ctx.settingsDataStore.data.first()[LEGACY_KEY_TOKEN])
    }

    @Test fun `migration is a no-op when no plaintext exists and encrypted is empty`() = runBlocking {
        store.clear()
        val repo = SettingsRepository(ctx, store)

        repo.migrateTokenToEncrypted()

        assertNull(store.read())
    }

    @Test fun `repeated reads hit the volatile cache and return the same value`() {
        store.write("cached")
        val a = store.read()
        val b = store.read()
        assertEquals("cached", a)
        assertEquals(a, b)
    }
}
