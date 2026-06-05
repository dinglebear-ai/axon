package com.axon.app

import androidx.activity.compose.setContent
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import com.axon.app.ui.document.DocumentScreen
import com.axon.app.ui.theme.AxonTheme
import org.junit.Rule
import org.junit.Test

class AppSmokeTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun appLaunchesAndOpensPrimaryNavigationSurfaces() {
        compose.onNodeWithText("Ask anything").assertIsDisplayed()

        compose.onNodeWithContentDescription("Launch operation").performClick()
        compose.onNodeWithContentDescription("Scrape").assertIsDisplayed()
        compose.onNodeWithContentDescription("Search").assertIsDisplayed()

        compose.onNodeWithContentDescription("Jobs").performClick()
        compose.onNodeWithText("Jobs").assertIsDisplayed()
        compose.onNodeWithText("Crawls").assertIsDisplayed()

        compose.onNodeWithContentDescription("Mgmt").performClick()
        compose.onNodeWithText("Config").assertIsDisplayed()
    }

    @Test fun documentRouteComposableRendersLoadingState() {
        compose.activity.setContent {
            AxonTheme {
                DocumentScreen(url = "https://example.com/docs")
            }
        }

        compose.onNodeWithText("Fetching document…").assertIsDisplayed()
    }
}
