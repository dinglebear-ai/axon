package com.axon.app

import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.util.Log
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.lifecycleScope
import com.axon.app.data.repository.RecentJob
import com.axon.app.data.util.SharedUrlExtractor
import com.axon.app.ui.nav.AxonNavGraph
import com.axon.app.ui.theme.AxonTheme
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

private const val TAG = "MainActivity"

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
        )
        setContent {
            AxonTheme(dark = true) {
                AxonNavGraph()
            }
        }
        if (savedInstanceState == null) {
            handleShareIntent(intent)
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleShareIntent(intent)
    }

    private fun handleShareIntent(intent: Intent?) {
        if (intent?.action != Intent.ACTION_SEND) return
        val sharedUrl = extractSharedUrl(intent)
        if (sharedUrl == null) {
            Toast.makeText(this, "Share a valid http or https URL to index.", Toast.LENGTH_LONG).show()
            return
        }
        submitSharedSite(sharedUrl)
    }

    private fun extractSharedUrl(intent: Intent): String? {
        val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)
        SharedUrlExtractor.firstHttpUrl(text)?.let { return it }

        val subject = intent.getCharSequenceExtra(Intent.EXTRA_SUBJECT)
        SharedUrlExtractor.firstHttpUrl(subject)?.let { return it }

        val clipData = intent.clipData ?: return null
        for (index in 0 until clipData.itemCount) {
            val item = clipData.getItemAt(index)
            SharedUrlExtractor.firstHttpUrl(item.text)?.let { return it }
            SharedUrlExtractor.firstHttpUrl(item.uri?.toString())?.let { return it }
        }
        return null
    }

    private fun submitSharedSite(url: String) {
        val container = (applicationContext as AxonApp).container
        lifecycleScope.launch {
            container.isReady.filter { it }.first()
            Toast.makeText(this@MainActivity, "Submitting site source...", Toast.LENGTH_SHORT).show()
            container.axonRepository.sourceSiteSubmit(url).fold(
                onSuccess = { jobId ->
                    container.recentJobs.add(
                        RecentJob(
                            jobId = jobId,
                            kind = "source",
                            target = url,
                            submittedAt = System.currentTimeMillis(),
                        ),
                    )
                    Toast.makeText(this@MainActivity, "Site indexing queued. Track it from Jobs.", Toast.LENGTH_LONG).show()
                },
                onFailure = { error ->
                    Log.w(TAG, "shared site source submit failed", error)
                    Toast
                        .makeText(
                            this@MainActivity,
                            error.message ?: "Site source submit failed",
                            Toast.LENGTH_LONG,
                        ).show()
                },
            )
        }
    }
}
