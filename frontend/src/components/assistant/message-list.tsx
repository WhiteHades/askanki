import { AnimatePresence, motion } from 'motion/react'
import { Bot, CircleAlert, RotateCcw, Sparkles, UserRound } from 'lucide-react'
import { useEffect, useRef } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

import { ScrollArea } from '@/components/ui/scroll-area'
import { ContextCard, LoadingDots, StreamText, TaskRows, ThinkingState, ToolChips } from '@/components/assistant/beautiful-primitives'
import { cn } from '@/lib/utils'
import type { CardContext, ChatMessage, RunState, RunStep } from '@/types'

export function MessageList({ messages, card, runState, steps, tools, error, onRetry, onSuggestion }: { messages: ChatMessage[]; card: CardContext; runState: RunState; steps: RunStep[]; tools: Array<{ id: string; label: string; detail?: string; status: RunStep['status'] }>; error?: string; onRetry?: () => void; onSuggestion: (text: string) => void }) {
  const scrollRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const viewport = scrollRef.current?.querySelector('[data-slot="scroll-area-viewport"]')
    viewport?.scrollTo({ top: viewport.scrollHeight, behavior: 'smooth' })
  }, [messages, runState, steps])

  return (
    <div ref={scrollRef} className="message-scroll">
      <ScrollArea className="h-full">
        <div className="message-column" role="log" aria-live="polite" aria-label="Conversation">
        <ContextCard context={card} />
        {messages.length === 0 && runState === 'idle' ? <EmptyThread onSuggestion={onSuggestion} /> : null}
        <AnimatePresence initial={false}>
          {messages.map((message) => <MessageBubble key={message.id} message={message} />)}
        </AnimatePresence>
        {runState !== 'idle' ? <ThinkingState steps={steps} active={runState === 'running' || runState === 'starting'} onCancel={undefined} /> : null}
        {runState === 'running' && !steps.length ? <LoadingDots label="Starting the local agent" /> : null}
        <TaskRows steps={steps.filter((step) => step.status === 'succeeded' || step.status === 'failed' || step.status === 'running')} />
        <ToolChips tools={tools} />
        {error ? <div className="error-state" role="alert"><CircleAlert size={15} /><span>{error}</span>{onRetry ? <button type="button" className="text-button" onClick={onRetry}><RotateCcw size={12} />Retry</button> : null}</div> : null}
        </div>
      </ScrollArea>
    </div>
  )
}

function MessageBubble({ message }: { message: ChatMessage }) {
  const isUser = message.role === 'user'
  const isError = message.role === 'error'
  return (
    <motion.article className={cn('message-row', isUser ? 'message-row-user' : 'message-row-assistant', isError && 'message-row-error')} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}>
      <div className="message-avatar" aria-hidden="true">{isUser ? <UserRound size={14} /> : isError ? <CircleAlert size={14} /> : <Bot size={14} />}</div>
      <div className={cn('message-body', isUser && 'message-body-user')}>
        <div className="message-meta"><span>{isUser ? 'You' : isError ? 'Runtime' : 'AskAnki'}</span><time>{new Date(message.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time></div>
        {isUser ? <p>{message.content}</p> : <div className="markdown-body"><ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>{message.streaming ? <StreamText content="" streaming /> : null}</div>}
      </div>
    </motion.article>
  )
}

function EmptyThread({ onSuggestion }: { onSuggestion: (text: string) => void }) {
  const suggestions = ['Explain this card', 'Give me a memory hook', 'Quiz me gently']
  return (
    <section className="empty-thread">
      <div className="empty-thread-mark"><Sparkles size={17} /></div>
      <h2>Study this card with AskAnki</h2>
      <p>Ask for an explanation, a translation, a memory cue, or a quick check of understanding.</p>
      <div className="suggestion-list">{suggestions.map((suggestion) => <button type="button" className="suggestion" key={suggestion} onClick={() => onSuggestion(suggestion)}>{suggestion}<span>↗</span></button>)}</div>
    </section>
  )
}
