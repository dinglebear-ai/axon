import type { WsServerMsg } from '@/lib/ws-protocol'
import type { MessageHandlerRefs, MessageHandlerSetters } from './handlers'
import { handleWsMessage, isRuntimeRelevantWsMessage } from './handlers'

export const RUNTIME_WS_MESSAGE_TYPES: ReadonlyArray<WsServerMsg['type']> = [
  'log',
  'file_content',
  'crawl_files',
  'crawl_progress',
  'command.start',
  'command.output.line',
  'command.output.json',
  'command.done',
  'command.error',
  'job.status',
  'job.progress',
  'artifact.list',
  'artifact.content',
  'job.cancel.response',
]

export function subscribeRuntimeMessages(input: {
  subscribeByTypes: (
    types: ReadonlyArray<WsServerMsg['type']>,
    handler: (msg: WsServerMsg) => void,
  ) => () => void
  refs: MessageHandlerRefs
  setters: MessageHandlerSetters
  handleMessage?: typeof handleWsMessage
}): () => void {
  const handleMessage = input.handleMessage ?? handleWsMessage
  return input.subscribeByTypes(RUNTIME_WS_MESSAGE_TYPES, (msg: WsServerMsg) => {
    if (!isRuntimeRelevantWsMessage(msg)) return
    handleMessage(msg, input.refs, input.setters)
  })
}
