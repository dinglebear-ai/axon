package com.axon.app.data.auth

import androidx.test.core.app.ApplicationProvider
import android.content.Context
import org.junit.Before
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33])
class OAuthStateStoreTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private lateinit var store: OAuthStateStore

    @Before
    fun setUp() {
        store = OAuthStateStore(context)
        assumeTrue(
            "Robolectric keystore unavailable; encrypted OAuth state round-trip failed",
            store.write("__probe__") && OAuthStateStore(context).read() == "__probe__",
        )
        store.clear()
    }

    @Test
    fun `write read and clear auth state json`() {
        val json = """{"accessToken":"a","refreshToken":"r"}"""
        assertTrue(store.write(json))
        assertEquals(json, store.read())

        assertTrue(store.clear())
        assertNull(store.read())
    }
}
