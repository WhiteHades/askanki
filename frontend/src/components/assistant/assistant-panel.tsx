import { History, MoreHorizontal, PanelRightClose, Sparkles, Trash2 } from 'lucide-react'

import { SettingsDialog } from '@/components/assistant/settings-dialog'
import { ChatComposer } from '@/components/assistant/chat-composer'
import { MessageList } from '@/components/assistant/message-list'
import { StatusPill } from '@/components/assistant/beautiful-primitives'
import { Button } from '@/components/ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { Separator } from '@/components/ui/separator'
import type { CardContext, ChatMessage, RunState, RunStep, RuntimeConfig } from '@/types'

export function AssistantPanel({ config, card, messages, draft, runState, steps, tools, error, onDraftChange, onSend, onStop, onClose, onClear, onConfigChange }: { config: RuntimeConfig | null; card: CardContext; messages: ChatMessage[]; draft: string; runState: RunState; steps: RunStep[]; tools: Array<{ id: string; label: string; detail?: string; status: RunStep['status'] }>; error?: string; onDraftChange: (value: string) => void; onSend: () => void; onStop?: () => void; onClose: () => void; onClear: () => void; onConfigChange: (config: RuntimeConfig) => void }) {
  return (
    <section className="assistant-panel" aria-label="AskAnki assistant">
      <header className="assistant-header">
        <div className="assistant-brand"><span className="assistant-mark"><Sparkles size={14} /></span><div><strong>AskAnki</strong><span>Card-aware study assistant</span></div></div>
        <div className="assistant-header-actions">
          <StatusPill tone={runState === 'idle' ? 'success' : 'accent'}>{config?.provider === 'codex' ? 'Codex' : 'OpenCode'}</StatusPill>
          <SettingsDialog config={config} onConfigChange={onConfigChange} onClearHistory={onClear} />
          <DropdownMenu><DropdownMenuTrigger asChild><Button type="button" variant="ghost" size="icon-sm" aria-label="More assistant actions"><MoreHorizontal size={15} /></Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuItem onSelect={onClear}><Trash2 size={13} />Clear conversation</DropdownMenuItem></DropdownMenuContent></DropdownMenu>
          <Button type="button" variant="ghost" size="icon-sm" aria-label="Close assistant" onClick={onClose}><PanelRightClose size={15} /></Button>
        </div>
      </header>
      <Separator />
      <MessageList messages={messages} card={card} runState={runState} steps={steps} tools={tools} error={error} onRetry={onSend} onSuggestion={onDraftChange} />
      <footer className="assistant-footer"><div className="footer-context"><History size={12} /><span>Local history · {config?.history_retention_days ?? 30} day retention</span></div><ChatComposer value={draft} onChange={onDraftChange} onSend={onSend} onStop={onStop} disabled={!card.noteId} running={runState !== 'idle'} /></footer>
    </section>
  )
}
