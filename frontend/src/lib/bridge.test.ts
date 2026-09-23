import { afterEach, describe, expect, it, vi } from 'vitest'

import { BridgeError, callBridge } from './bridge'

afterEach(() => {
  delete window.ankiAskAnki
})

describe('callBridge', () => {
  it('returns a successful result', async () => {
    window.ankiAskAnki = {
      call: vi.fn().mockResolvedValue({ ok: true, result: { value: 42 } }),
    }

    await expect(callBridge<{ value: number }>('ping')).resolves.toEqual({ value: 42 })
    expect(window.ankiAskAnki.call).toHaveBeenCalledWith('ping', {})
  })

  it('maps a bridge failure to BridgeError', async () => {
    window.ankiAskAnki = {
      call: vi.fn().mockResolvedValue({ ok: false, error: { code: 'blocked', message: 'Denied' } }),
    }

    await expect(callBridge('agent_run')).rejects.toEqual(new BridgeError('blocked', 'Denied'))
  })

  it('fails when the Anki bridge is absent', async () => {
    await expect(callBridge('ping')).rejects.toMatchObject({ code: 'bridge_unavailable' })
  })
})
