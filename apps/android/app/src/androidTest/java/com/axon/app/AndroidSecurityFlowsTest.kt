package com.axon.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.axon.app.core.api.AxonClient
import com.axon.app.core.api.panelEnv
import com.axon.app.feature.ask.inferFabIngestSource
import com.axon.app.ui.ingest.IngestSource
import com.axon.app.feature.settings.validateAxonServerUrl
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AndroidSecurityFlowsTest {
    @Test fun fabIngestRejectsGithubLookalikeHostOnDeviceClasspath() {
        val inferred = inferFabIngestSource("https://github.com.attacker.com/owner/repo")

        assertTrue(inferred.isFailure)
    }

    @Test fun fabIngestAcceptsGithubShorthandOnDeviceClasspath() {
        val inferred = inferFabIngestSource("github/owner/repo")

        assertEquals(IngestSource.Github, inferred.getOrThrow())
    }

    @Test fun serverUrlPolicyRejectsPublicCleartextOnDeviceClasspath() {
        val result = runCatching { validateAxonServerUrl("http://axon.example.com") }

        assertTrue(result.isFailure)
    }

    @Test fun panelEnvRequiresUnlockBeforeNetworkRequest() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(
                MockResponse()
                    .setBody("""{"path":"~/.axon/.env","raw_env":"GITHUB_TOKEN=secret","restart_required":false}""")
                    .addHeader("Content-Type", "application/json"),
            )
            val client = AxonClient(
                baseUrl = server.url("/").toString().trimEnd('/'),
                token = "api-token",
            )

            val result = client.panelEnv()

            assertTrue(result.isFailure)
            assertEquals(0, server.requestCount)
        } finally {
            server.shutdown()
        }
    }
}
