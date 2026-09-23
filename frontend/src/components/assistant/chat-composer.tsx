import { ArrowUp, LoaderCircle, Square } from 'lucide-react'
import { useEffect, useRef } from 'react'

import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/lib/utils'

export function ChatComposer({ value, onChange, onSend, onStop, disabled, running }: { value: string; onChange: (value: string) => void; onSend: () => void; onStop?: () => void; disabled?: boolean; running?: boolean }) {
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  useEffect(() => {
    const element = textareaRef.current
    if (!element) return
    element.style.height = 'auto'
    element.style.height = `${Math.min(element.scrollHeight, 112)}px`
  }, [value])

  return (
    <div className="composer-wrap">
      <div className={cn('composer', disabled && 'composer-disabled')}>
        <Textarea ref={textareaRef} value={value} onChange={(event) => onChange(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); if (!disabled && value.trim()) onSend() } }} placeholder="Ask about this card…" aria-label="Ask about this card" rows={1} disabled={disabled || running} className="composer-textarea" />
        <div className="composer-footer"><span className="composer-hint">Enter to send · Shift + Enter for a new line</span>{running ? <Button type="button" size="icon-sm" variant="outline" aria-label="Stop run" onClick={onStop}><Square size={13} fill="currentColor" /></Button> : <Button type="button" size="icon-sm" aria-label="Send message" disabled={disabled || !value.trim()} onClick={onSend}><ArrowUp size={15} /></Button>}</div>
      </div>
      {running ? <div className="composer-running"><LoaderCircle size={12} className="spin" />Local agent is working</div> : null}
    </div>
  )
}
