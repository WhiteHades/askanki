import { AnimatePresence, motion } from 'motion/react'
import { Brain, Check, ChevronDown, CircleAlert, CircleX, FileCode2, LoaderCircle, Sparkles, Wrench } from 'lucide-react'
import { useState } from 'react'

import { cn } from '@/lib/utils'
import type { CardContext, RunStep } from '@/types'

export function StatusPill({ tone = 'neutral', children }: { tone?: 'neutral' | 'accent' | 'success' | 'warning' | 'danger'; children: React.ReactNode }) {
  return <span className={cn('status-pill', `status-pill-${tone}`)}>{children}</span>
}

export function LoadingDots({ label = 'Thinking' }: { label?: string }) {
  return (
    <span className="inline-flex items-center gap-2 text-xs text-ink-2" role="status" aria-live="polite">
      <span className="loading-dots" aria-hidden="true"><i /><i /><i /></span>
      <span>{label}</span>
    </span>
  )
}

export function ThinkingState({ steps, active, onCancel }: { steps: RunStep[]; active: boolean; onCancel?: () => void }) {
  const [expanded, setExpanded] = useState(true)
  return (
    <section className="thinking-state" aria-label="Agent activity">
      <div className="thinking-header">
        <button type="button" className="thinking-trigger" aria-expanded={expanded} onClick={() => setExpanded((value) => !value)}>
          <span className={cn('thinking-icon', active && 'thinking-icon-active')}>
            {active ? <LoaderCircle size={15} className="spin" /> : <Brain size={15} />}
          </span>
          <span className="thinking-title">{active ? 'Working on your request' : 'Run details'}</span>
          <ChevronDown size={14} className={cn('thinking-chevron', expanded && 'thinking-chevron-open')} />
        </button>
        {active && onCancel ? <button type="button" className="text-button" onClick={onCancel}>Stop</button> : null}
      </div>
      <AnimatePresence initial={false}>
        {expanded ? (
          <motion.div className="thinking-steps" initial={{ height: 0, opacity: 0 }} animate={{ height: 'auto', opacity: 1 }} exit={{ height: 0, opacity: 0 }} transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}>
            {steps.length ? steps.map((step) => <ThinkingRow key={step.id} step={step} />) : <LoadingDots label="Preparing local context" />}
          </motion.div>
        ) : null}
      </AnimatePresence>
    </section>
  )
}

function ThinkingRow({ step }: { step: RunStep }) {
  const icon = step.status === 'succeeded' ? <Check size={13} /> : step.status === 'failed' ? <CircleX size={13} /> : step.status === 'running' ? <LoaderCircle size={13} className="spin" /> : <span className="step-dot" />
  return (
    <div className={cn('thinking-row', `thinking-row-${step.status}`)}>
      <span className="thinking-row-icon">{icon}</span>
      <span className="thinking-row-label">{step.label}</span>
      {step.detail ? <span className="thinking-row-detail">{step.detail}</span> : null}
    </div>
  )
}

export function StreamText({ content, streaming }: { content: string; streaming?: boolean }) {
  return (
    <div className="stream-text">
      {content || <span className="stream-placeholder">Waiting for the first token…</span>}
      {streaming ? <span className="stream-caret" aria-label="Streaming" /> : null}
    </div>
  )
}

export function ContextCard({ context }: { context: CardContext }) {
  const excerpt = context.text.length > 260 ? `${context.text.slice(0, 260)}…` : context.text
  const metadata = [context.imageCount ? `${context.imageCount} image${context.imageCount === 1 ? '' : 's'}` : null, context.hasMath ? 'Math' : null, context.hasCode ? 'Code' : null].filter(Boolean).join(' · ')
  return (
    <article className="context-card">
      <div className="context-card-header">
        <div className="context-card-title"><Sparkles size={13} /><span>Current card</span></div>
        <StatusPill tone="accent">{context.noteId ? `Note ${context.noteId}` : 'No active note'}</StatusPill>
      </div>
      <p className="context-card-excerpt">{excerpt || 'No card text is available in this surface yet.'}</p>
      <div className="context-card-footer"><span>{metadata || 'Text context'}</span><span className="context-card-ready">Ready for a local run</span></div>
    </article>
  )
}

export function TaskRows({ steps }: { steps: RunStep[] }) {
  if (!steps.length) return null
  return (
    <div className="task-list" role="group" aria-label="Agent tasks">
      {steps.map((step) => (
        <div className="task-row" key={step.id}>
          <span className={cn('task-status', `task-status-${step.status}`)}>
            {step.status === 'succeeded' ? <Check size={12} /> : step.status === 'failed' ? <CircleX size={12} /> : step.status === 'running' ? <LoaderCircle size={12} className="spin" /> : <span className="step-dot" />}
          </span>
          <span className="task-label">{step.label}</span>
          <span className="task-detail">{step.detail}</span>
        </div>
      ))}
    </div>
  )
}

export function ToolChips({ tools }: { tools: Array<{ id: string; label: string; detail?: string; status: RunStep['status'] }> }) {
  if (!tools.length) return null
  return (
    <div className="tool-chips" role="group" aria-label="Tool activity">
      {tools.map((tool) => <span className={cn('tool-chip', `tool-chip-${tool.status}`)} key={tool.id}><Wrench size={11} />{tool.label}{tool.detail ? <span>{tool.detail}</span> : null}</span>)}
    </div>
  )
}

export function ApprovalCard({ summary, detail, onDecision }: { summary: string; detail?: string; onDecision: (approved: boolean) => void }) {
  const [decision, setDecision] = useState<'pending' | 'approved' | 'denied'>('pending')
  const decide = (approved: boolean) => {
    setDecision(approved ? 'approved' : 'denied')
    onDecision(approved)
  }
  return (
    <section className={cn('approval-card', decision !== 'pending' && 'approval-card-settled')} aria-live="polite">
      <div className="approval-icon">{decision === 'approved' ? <Check size={15} /> : decision === 'denied' ? <CircleX size={15} /> : <CircleAlert size={15} />}</div>
      <div className="approval-copy"><strong>{decision === 'approved' ? 'Approved' : decision === 'denied' ? 'Declined' : 'Permission needed'}</strong><span>{summary}</span>{detail ? <small>{detail}</small> : null}</div>
      {decision === 'pending' ? <div className="approval-actions"><button type="button" className="button button-quiet" onClick={() => decide(false)}>Decline</button><button type="button" className="button button-primary" onClick={() => decide(true)}>Allow</button></div> : null}
    </section>
  )
}

export function MarkdownIcon({ kind }: { kind: 'code' | 'tool' }) {
  return kind === 'code' ? <FileCode2 size={13} /> : <Wrench size={13} />
}
