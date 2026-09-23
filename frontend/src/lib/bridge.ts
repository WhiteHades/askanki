import type { RuntimeConfig } from '@/types'

type BridgeResponse<T> = {
  ok: boolean
  result?: T
  error?: {
    code: string
    message: string
  }
}

type AskAnkiBridge = {
  call: (action: string, payload?: Record<string, unknown>) => Promise<BridgeResponse<unknown>>
}

declare global {
  interface Window {
    ankiAskAnki?: AskAnkiBridge
  }
}

export class BridgeError extends Error {
  readonly code: string

  constructor(code: string, message: string) {
    super(message)
    this.code = code
  }
}

export function hasBridge() {
  return typeof window !== 'undefined' && Boolean(window.ankiAskAnki)
}

export async function callBridge<T>(action: string, payload: Record<string, unknown> = {}) {
  const bridge = typeof window === 'undefined' ? undefined : window.ankiAskAnki
  if (!bridge) {
    throw new BridgeError('bridge_unavailable', 'Open AskAnki inside Anki to connect the local runtime.')
  }

  const response = await bridge.call(action, payload)
  if (!response.ok) {
    throw new BridgeError(response.error?.code ?? 'bridge_error', response.error?.message ?? 'The local runtime returned an error.')
  }
  return response.result as T
}

export async function getRuntimeConfig() {
  return callBridge<{ config: RuntimeConfig; note_id: string | null }>('config_get')
}
