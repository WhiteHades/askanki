import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { Sparkles } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'

import { AssistantPanel } from '@/components/assistant/assistant-panel'
import { Button } from '@/components/ui/button'
import { TooltipProvider } from '@/components/ui/tooltip'
import { callBridge, getRuntimeConfig, hasBridge } from '@/lib/bridge'
import { getCardContext, observeCardContext } from '@/lib/card-context'
import type { AgentRunResult, CardContext, ChatMessage, HistoryEntry, RunState, RunStep, RuntimeConfig } from '@/types'

const emptyCard: CardContext = { noteId: null, text: '', imageCount: 0, hasImages: false, hasMath: false, hasCode: false }

function messageId() {
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`
}

function App() {
  const reduceMotion = useReducedMotion()
  const [open, setOpen] = useState(false)
  const [config, setConfig] = useState<RuntimeConfig | null>(null)
  const [noteId, setNoteId] = useState<string | null>(null)
  const [card, setCard] = useState<CardContext>(emptyCard)
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [draft, setDraft] = useState('')
  const [runState, setRunState] = useState<RunState>('idle')
  const [steps, setSteps] = useState<RunStep[]>([])
  const [tools, setTools] = useState<AgentRunResult['tools']>([])
  const [error, setError] = useState<string | undefined>()
  const cardSignatureRef = useRef('')

  const loadHistory = useCallback(async (id: string) => {
    try {
      const result = await callBridge<{ entries: HistoryEntry[] }>('history_load', { note_id: id, limit: 8 })
      setMessages(result.entries.filter((entry) => entry.role === 'user' || entry.role === 'assistant' || entry.role === 'error').map((entry) => ({ id: entry.id, role: entry.role as ChatMessage['role'], content: entry.content, createdAt: entry.created_at * 1000 })))
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : 'Could not load local history.')
    }
  }, [])

  useEffect(() => {
    let active = true
    if (!hasBridge()) {
      setError('Open AskAnki inside Anki to connect the local runtime.')
      return () => { active = false }
    }
    void getRuntimeConfig().then((result) => {
      if (!active) return
      setConfig(result.config)
      setNoteId(result.note_id)
      setCard(getCardContext(result.note_id))
    }).catch((bridgeError) => {
      if (active) setError(bridgeError instanceof Error ? bridgeError.message : 'Could not connect to Anki.')
    })
    return () => { active = false }
  }, [])

  useEffect(() => {
    if (!noteId) return
    const context = getCardContext(noteId)
    setCard(context)
    cardSignatureRef.current = `${noteId}:${context.text}:${context.imageCount}`
    void loadHistory(noteId)
    return observeCardContext(noteId, (next) => {
      const signature = `${noteId}:${next.text}:${next.imageCount}`
      setCard(next)
      if (signature !== cardSignatureRef.current) {
        cardSignatureRef.current = signature
        setMessages([])
        setDraft('')
        setError(undefined)
        void loadHistory(noteId)
      }
    })
  }, [loadHistory, noteId])

  const send = useCallback(async (override?: string) => {
    const prompt = (override ?? draft).trim()
    if (!prompt || !noteId || runState !== 'idle') return
    const userMessage: ChatMessage = { id: messageId(), role: 'user', content: prompt, createdAt: Date.now() }
    setMessages((current) => [...current, userMessage])
    setDraft('')
    setError(undefined)
    setRunState('starting')
    setSteps([{ id: 'context', label: 'Read current card context', status: 'succeeded' }, { id: 'agent', label: `Start ${config?.provider === 'codex' ? 'Codex' : 'OpenCode'}`, status: 'running' }])
    try {
      await callBridge('history_append', { note_id: noteId, role: 'user', content: prompt })
      const result = await callBridge<AgentRunResult>('agent_run', { note_id: noteId, prompt, card_context: card, history: messages, provider: config?.provider, workspace: config?.workspace })
      setSteps(result.steps ?? [{ id: 'agent', label: 'Local agent finished', status: 'succeeded' }])
      setTools(result.tools ?? [])
      setMessages((current) => [...current, { id: messageId(), role: 'assistant', content: result.text, createdAt: Date.now() }])
      await callBridge('history_append', { note_id: noteId, role: 'assistant', content: result.text })
    } catch (runError) {
      const message = runError instanceof Error ? runError.message : 'The local agent could not complete this run.'
      setError(message)
      setSteps([{ id: 'agent', label: 'Run failed', detail: 'Review the runtime message', status: 'failed' }])
    } finally {
      setRunState('idle')
    }
  }, [card, config, draft, messages, noteId, runState])

  const stop = useCallback(async () => {
    if (!noteId || runState === 'idle') return
    setRunState('stopping')
    try {
      await callBridge('agent_cancel', { note_id: noteId })
    } catch (stopError) {
      setError(stopError instanceof Error ? stopError.message : 'Could not stop the local run.')
    } finally {
      setRunState('idle')
    }
  }, [noteId, runState])

  const clear = useCallback(async () => {
    setMessages([])
    setError(undefined)
    if (!noteId) return
    try {
      await callBridge('history_clear', { note_id: noteId })
    } catch (clearError) {
      setError(clearError instanceof Error ? clearError.message : 'Could not clear local history.')
    }
  }, [noteId])

  const transition = reduceMotion ? { duration: 0 } : { type: 'spring' as const, stiffness: 420, damping: 34, mass: 0.7 }
  const panel = <AssistantPanel config={config} card={card} messages={messages} draft={draft} runState={runState} steps={steps} tools={tools ?? []} error={error} onDraftChange={setDraft} onSend={() => void send()} onStop={() => void stop()} onClose={() => setOpen(false)} onClear={() => void clear()} onConfigChange={setConfig} />

  return (
    <TooltipProvider delayDuration={350}>
      <div className="app-surface">
        <AnimatePresence mode="wait" initial={false}>
          {open ? <motion.div key="panel" className="assistant-positioner" initial={{ opacity: 0, scale: 0.96, y: 8 }} animate={{ opacity: 1, scale: 1, y: 0 }} exit={{ opacity: 0, scale: 0.98, y: 6 }} transition={transition}>{panel}</motion.div> : <motion.div key="launcher" className="launcher-positioner" initial={{ opacity: 0, scale: 0.9 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.9 }} transition={transition}><Button type="button" className="assistant-launcher" aria-label="Open AskAnki" onClick={() => setOpen(true)}><Sparkles size={17} /><span>AskAnki</span></Button></motion.div>}
        </AnimatePresence>
      </div>
    </TooltipProvider>
  )
}

export default App
