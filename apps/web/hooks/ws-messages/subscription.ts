import type { WsServerMsg } from '@/lib/ws-protocol'
import type { MessageHandlerRefs, MessageHandlerSetters } from './handlers'
import { handleWsMessage, isRuntimeRelevantWsMessage, RUNTIME_EVENT_TYPES } from './handlers'

/** Re-export so existing consumers can keep importing from this module. */
export const RUNTIME_WS_MESSAGE_TYPES = RUNTIME_EVENT_TYPES

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
