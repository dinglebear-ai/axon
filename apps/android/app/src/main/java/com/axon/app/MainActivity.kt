package com.axon.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import com.axon.app.ui.nav.AxonNavGraph
import com.axon.app.ui.theme.AxonTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            AxonTheme {
                AxonNavGraph()
            }
        }
    }
}
