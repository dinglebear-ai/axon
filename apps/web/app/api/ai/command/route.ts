import { createGateway } from '@ai-sdk/gateway'
import {
  createUIMessageStream,
  createUIMessageStreamResponse,
  generateText,
  type LanguageModel,
  streamText,
  tool,
  type UIMessageStreamWriter,
} from 'ai'
import type { NextRequest } from 'next/server'
import { createSlateEditor, nanoid, type SlateEditor } from 'platejs'
import { z } from 'zod'
import { BaseEditorKit } from '@/components/editor/editor-base-kit'
import type { ChatMessage, ToolName } from '@/components/editor/use-chat'
import { markdownJoinerTransform } from '@/lib/markdown-joiner-transform'
import { apiError, makeErrorId } from '@/lib/server/api-error'
import { logError } from '@/lib/server/logger'

import {
  buildEditTableMultiCellPrompt,
  getChooseToolPrompt,
  getCommentPrompt,
  getEditPrompt,
  getGeneratePrompt,
} from './prompt'

/** Default model for tool-choosing / complex tasks (e.g. selection edits, comments). */
const MODEL_TOOL_CHOOSER = 'google/gemini-2.5-flash'
/** Default model for generation / simple edits. */
const MODEL_GENERATE = 'openai/gpt-4o-mini'

const CommandBodySchema = z.object({
  ctx: z.object({
    children: z.array(z.any()),
    selection: z.any().nullable().optional(),
    toolName: z.string().optional(),
  }),
  messages: z.array(z.any()).min(1),
  model: z.string().optional(),
})

export async function POST(req: NextRequest) {
  try {
    const body = await req.json()
    const parsed = CommandBodySchema.safeParse(body)

    if (!parsed.success) {
      return apiError(400, parsed.error.issues[0]?.message ?? 'Invalid request payload')
    }

    const { ctx, messages: messagesRaw, model } = parsed.data
    const { children, selection, toolName: toolNameParam } = ctx

    const editor = createSlateEditor({
      plugins: BaseEditorKit,
      selection,
      value: children,
    })

    const apiKey = process.env.AI_GATEWAY_API_KEY
    if (!apiKey) {
      return apiError(401, 'Missing AI Gateway API key', { code: 'ai_command_no_key' })
    }

    const isSelecting = editor.api.isExpanded()
    const gatewayProvider = createGateway({ apiKey })
    const enumOptions = isSelecting ? ['generate', 'edit', 'comment'] : ['generate', 'comment']
    const validToolNames = new Set<ToolName>(['generate', 'edit', 'comment'])
    let validatedToolName =
      typeof toolNameParam === 'string' && validToolNames.has(toolNameParam as ToolName)
        ? (toolNameParam as ToolName)
        : undefined

    const stream = createUIMessageStream<ChatMessage>({
      execute: async ({ writer }) => {
        let toolName = validatedToolName

        if (!toolName) {
          const prompt = getChooseToolPrompt({
            isSelecting,
            messages: messagesRaw,
          })

          const modelId = model || MODEL_TOOL_CHOOSER

          const { text: rawToolName } = await generateText({
            model: gatewayProvider(modelId),
            prompt,
            system: `Respond with exactly one word from: ${enumOptions.join(', ')}. No other text.`,
          })
          const AIToolName = (
            enumOptions.includes(rawToolName.trim()) ? rawToolName.trim() : enumOptions[0]
          ) as ToolName

          writer.write({
            data: AIToolName as ToolName,
            type: 'data-toolName',
          })

          toolName = AIToolName
          validatedToolName = AIToolName
        } else {
          writer.write({
            data: toolName,
            type: 'data-toolName',
          })
        }

        const stream = streamText({
          experimental_transform: markdownJoinerTransform(),
          model: gatewayProvider(model || MODEL_GENERATE),
          // Not used
          prompt: '',
          tools: {
            comment: getCommentTool(editor, {
              messagesRaw,
              model: gatewayProvider(model || MODEL_TOOL_CHOOSER),
              writer,
            }),
            table: getTableTool(editor, {
              messagesRaw,
              model: gatewayProvider(model || MODEL_TOOL_CHOOSER),
              writer,
            }),
          },
          prepareStep: async (step) => {
            if (toolName === 'comment') {
              return {
                ...step,
                toolChoice: { toolName: 'comment', type: 'tool' },
              }
            }

            if (toolName === 'edit') {
              const [editPrompt, editType] = getEditPrompt(editor, {
                isSelecting,
                messages: messagesRaw,
              })

              // Table editing uses the table tool
              if (editType === 'table') {
                return {
                  ...step,
                  toolChoice: { toolName: 'table', type: 'tool' },
                }
              }

              return {
                ...step,
                activeTools: [],
                model:
                  editType === 'selection'
                    ? //The selection task is more challenging, so we chose to use Gemini 2.5 Flash.
                      gatewayProvider(model || MODEL_TOOL_CHOOSER)
                    : gatewayProvider(model || MODEL_GENERATE),
                messages: [
                  {
                    content: editPrompt,
                    role: 'user',
                  },
                ],
              }
            }

            if (toolName === 'generate') {
              const generatePrompt = getGeneratePrompt(editor, {
                isSelecting,
                messages: messagesRaw,
              })

              return {
                ...step,
                activeTools: [],
                messages: [
                  {
                    content: generatePrompt,
                    role: 'user',
                  },
                ],
                model: gatewayProvider(model || MODEL_GENERATE),
              }
            }
          },
        })

        writer.merge(stream.toUIMessageStream({ sendFinish: false }))
      },
    })

    return createUIMessageStreamResponse({ stream })
  } catch (error) {
    const errorId = makeErrorId('ai-command')
    const message = error instanceof Error ? error.message : String(error)
    logError('api.ai.command.unhandled_error', { errorId, message })
    return apiError(500, 'Failed to process AI request', {
      code: 'ai_command_internal',
      errorId,
    })
  }
}

const getCommentTool = (
  editor: SlateEditor,
  {
    messagesRaw,
    model,
    writer,
  }: {
    messagesRaw: ChatMessage[]
    model: LanguageModel
    writer: UIMessageStreamWriter<ChatMessage>
  },
) =>
  tool({
    description: 'Comment on the content',
    inputSchema: z.object({}),
    execute: async () => {
      const commentDataId = nanoid()
      try {
        const { text } = await generateText({
          model,
          prompt: getCommentPrompt(editor, { messages: messagesRaw }),
          system:
            'Return a JSON array of comment objects. Each object must have: blockId (string), comment (string), content (string). Only return the JSON array, no other text.',
        })
        const comments = JSON.parse(text) as Array<{
          blockId: string
          comment: string
          content: string
        }>
        for (const comment of comments) {
          writer.write({
            id: commentDataId,
            data: { comment, status: 'streaming' },
            type: 'data-comment',
          })
        }
      } catch (err) {
        logError('api.ai.command.comment_tool_error', {
          message: err instanceof Error ? err.message : String(err),
        })
      } finally {
        writer.write({
          id: nanoid(),
          data: { comment: null, status: 'finished' },
          type: 'data-comment',
        })
      }
    },
  })

const getTableTool = (
  editor: SlateEditor,
  {
    messagesRaw,
    model,
    writer,
  }: {
    messagesRaw: ChatMessage[]
    model: LanguageModel
    writer: UIMessageStreamWriter<ChatMessage>
  },
) =>
  tool({
    description: 'Edit table cells',
    inputSchema: z.object({}),
    execute: async () => {
      try {
        const { text } = await generateText({
          model,
          prompt: buildEditTableMultiCellPrompt(editor, messagesRaw),
          system:
            'Return a JSON array of cell update objects. Each object must have: id (string cell id), content (string new content). Only return the JSON array, no other text.',
        })
        const updates = JSON.parse(text) as Array<{ id: string; content: string }>
        for (const cellUpdate of updates) {
          writer.write({
            id: nanoid(),
            data: { cellUpdate, status: 'streaming' },
            type: 'data-table',
          })
        }
      } catch (err) {
        logError('api.ai.command.table_tool_error', {
          message: err instanceof Error ? err.message : String(err),
        })
      } finally {
        writer.write({
          id: nanoid(),
          data: { cellUpdate: null, status: 'finished' },
          type: 'data-table',
        })
      }
    },
  })
