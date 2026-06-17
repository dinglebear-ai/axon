package com.axon.app.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.v2.runComposeUiTest
import com.axon.app.ui.ask.AskPromptBar
import com.axon.app.ui.ask.ConversationMode
import com.axon.app.ui.common.AxonCompactTabs
import com.axon.app.ui.common.AuroraStatusDot
import com.axon.app.ui.common.DotState
import com.axon.app.ui.options.components.HeadersField
import com.axon.app.ui.theme.AxonTheme
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, sdk = [33])
class AuroraPrimitiveSemanticsTest {

    @Test
    fun `prompt bar exposes editable input and enabled send semantics`() = runComposeUiTest {
        var value by mutableStateOf("")
        var sent = false

        setContent {
            AxonTheme {
                AskPromptBar(
                    value = value,
                    onValueChange = { value = it },
                    onSend = { sent = true },
                    loading = false,
                    placeholder = "Ask anything",
                    mode = ConversationMode.Ask,
                    onModeChange = {},
                    attachments = emptyList(),
                    onAttachClick = {},
                    onRemoveAttachment = {},
                )
            }
        }

        onNodeWithContentDescription("Ask prompt").performTextInput("hello")
        waitForIdle()

        onNodeWithContentDescription("Send message").assertIsEnabled().performClick()
        runOnIdle { check(sent) }
    }

    @Test
    fun `headers field keeps sensitive value hidden until explicit reveal`() = runComposeUiTest {
        setContent {
            AxonTheme {
                HeadersField(
                    headers = listOf("Authorization: Bearer secret-token"),
                    onChange = {},
                )
            }
        }

        onAllNodesWithText("Bearer secret-token").assertCountEquals(0)
        onAllNodesWithContentDescription("Show value").assertCountEquals(0)
        onNodeWithContentDescription("Show header value").assertIsDisplayed().performClick()
        waitForIdle()

        onNodeWithContentDescription("Hide header value").assertIsDisplayed()
    }

    @Test
    fun `compact tabs expose selected state`() = runComposeUiTest {
        setContent {
            AxonTheme {
                AxonCompactTabs(
                    tabs = listOf("One", "Two"),
                    selectedIndex = 1,
                    onTabSelected = {},
                )
            }
        }

        onNodeWithText("Two").assertIsSelected()
    }

    @Test
    fun `dot-only status keeps an accessible description`() = runComposeUiTest {
        setContent {
            AxonTheme {
                AuroraStatusDot(DotState.Running)
            }
        }

        onNodeWithContentDescription("Syncing").assertIsDisplayed()
    }
}
