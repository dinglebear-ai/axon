'use client'

import { z } from 'zod'
import { AcpConfigOption as AcpConfigOptionSchema } from '@/lib/pulse/types'

export const UnknownRecordSchema = z.record(z.string(), z.unknown())
export const WsUsageSchema = z
  .object({
    input_tokens: z.number().int().nonnegative().optional(),
    output_tokens: z.number().int().nonnegative().optional(),
    total_tokens: z.number().int().nonnegative().optional(),
    cache_creation_input_tokens: z.number().int().nonnegative().optional(),
    cache_read_input_tokens: z.number().int().nonnegative().optional(),
  })
  .strict()
export const AssistantDeltaSchema = z
  .object({
    type: z.literal('assistant_delta'),
    delta: z.string().default(''),
    usage: WsUsageSchema.optional(),
    tool_locations: z.array(z.string()).optional(),
    tool_call_id: z.string().optional(),
  })
  .passthrough()
export const UsageUpdateSchema = z
  .object({
    type: z.literal('usage_update'),
    usage: WsUsageSchema,
  })
  .passthrough()
export const ThinkingContentSchema = z
  .object({
    type: z.literal('thinking_content'),
    content: z.string().default(''),
  })
  .passthrough()
export const SessionFallbackSchema = z
  .object({
    type: z.literal('session_fallback'),
    old_session_id: z.string().default(''),
    new_session_id: z.string().default(''),
  })
  .passthrough()
export const ResultSchema = z
  .object({
    type: z.literal('result'),
    session_id: z.string().optional(),
  })
  .passthrough()
export const ErrorSchema = z
  .object({
    type: z.literal('error'),
    message: z.string().optional(),
    error: z.string().optional(),
  })
  .passthrough()
export const ToolUseSchema = z
  .object({
    type: z.literal('tool_use'),
    tool_call_id: z.string().default(''),
    tool_name: z.string().default('unknown'),
    tool_input: UnknownRecordSchema.default({}),
  })
  .passthrough()
export const ToolUseUpdateSchema = z
  .object({
    type: z.literal('tool_use_update'),
    tool_call_id: z.string().default(''),
    tool_status: z.string().default(''),
    tool_content: z.string().default(''),
  })
  .passthrough()
export const ConfigOptionsUpdateSchema = z
  .object({
    type: z.enum(['config_options_update', 'config_option_update']),
    configOptions: z.array(AcpConfigOptionSchema),
  })
  .passthrough()
export const CommandsUpdateSchema = z
  .object({
    type: z.literal('commands_update'),
    commands: z.array(
      z.object({
        name: z.string(),
        description: z.string().optional(),
      }),
    ),
  })
  .passthrough()
export const AcpResumeResultSchema = z
  .object({
    type: z.literal('acp_resume_result'),
    ok: z.boolean().optional(),
    replayed: z.number().int().nonnegative().optional(),
    session_id: z.string().optional(),
    reason: z.string().optional(),
  })
  .passthrough()
export const PermissionRequestSchema = z
  .object({
    type: z.literal('permission_request'),
    session_id: z.string(),
    tool_call_id: z.string(),
    options: z.array(z.string()),
  })
  .passthrough()
export const SessionInfoUpdateSchema = z
  .object({
    type: z.literal('session_info_update'),
    session_id: z.string(),
  })
  .passthrough()
export const EditorUpdateSchema = z
  .object({
    type: z.literal('editor_update'),
    content: z.string(),
    operation: z.enum(['replace', 'append']).optional(),
  })
  .passthrough()
export const SynthesisDeltaSchema = z
  .object({
    type: z.literal('synthesis_delta'),
    text: z.string().default(''),
  })
  .passthrough()
export const CommandOutputJsonEnvelopeSchema = z.object({
  type: z.literal('command.output.json'),
  data: z
    .object({
      ctx: z
        .object({
          mode: z.string(),
        })
        .passthrough(),
      data: z.unknown(),
    })
    .passthrough(),
})
