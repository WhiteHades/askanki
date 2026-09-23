import { Settings2, Trash2 } from 'lucide-react'
import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { callBridge } from '@/lib/bridge'
import type { RuntimeConfig } from '@/types'

export function SettingsDialog({ config, onConfigChange, onClearHistory }: { config: RuntimeConfig | null; onConfigChange: (config: RuntimeConfig) => void; onClearHistory: () => void }) {
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState(config)
  const [saving, setSaving] = useState(false)
  const [message, setMessage] = useState('')
  if (!draft) return null

  const handleOpenChange = (nextOpen: boolean) => {
    if (nextOpen && config) setDraft(config)
    setOpen(nextOpen)
  }

  const save = async () => {
    setSaving(true)
    setMessage('')
    try {
      const result = await callBridge<{ config: RuntimeConfig }>('config_save', { config: draft })
      onConfigChange(result.config)
      setMessage('Saved locally')
      setOpen(false)
    } catch (error) {
      setMessage(error instanceof Error ? error.message : 'Could not save settings')
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild><Button type="button" variant="ghost" size="icon-sm" aria-label="Open settings"><Settings2 size={15} /></Button></DialogTrigger>
      <DialogContent className="settings-dialog">
        <DialogHeader><DialogTitle>AskAnki settings</DialogTitle><DialogDescription>Local runtime preferences stay on this machine.</DialogDescription></DialogHeader>
        <div className="settings-fields">
          <div className="field-group"><Label htmlFor="provider">Local agent</Label><Select value={draft.provider} onValueChange={(value: 'opencode' | 'codex') => setDraft({ ...draft, provider: value })}><SelectTrigger id="provider"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="opencode">OpenCode</SelectItem><SelectItem value="codex">Codex</SelectItem></SelectContent></Select></div>
          <div className="field-group"><Label htmlFor="workspace">Workspace folder</Label><Input id="workspace" value={draft.workspace} onChange={(event) => setDraft({ ...draft, workspace: event.target.value })} placeholder="Choose a folder for local agents" /></div>
          <div className="field-group"><Label htmlFor="retention">History retention (days)</Label><Input id="retention" type="number" min={0} max={3650} value={draft.history_retention_days} onChange={(event) => setDraft({ ...draft, history_retention_days: Number(event.target.value) })} /></div>
          <div className="field-group"><Label htmlFor="instruction">Assistant instruction</Label><textarea id="instruction" className="settings-textarea" value={draft.system_instruction} onChange={(event) => setDraft({ ...draft, system_instruction: event.target.value })} rows={4} /></div>
        </div>
        <Separator />
        <div className="settings-danger"><div><strong>Local history</strong><span>Stored under Anki user files for this note.</span></div><Button type="button" variant="outline" size="sm" onClick={onClearHistory}><Trash2 size={13} />Clear current note</Button></div>
        {message ? <p className="settings-message" role="status">{message}</p> : null}
        <DialogFooter><Button type="button" variant="ghost" onClick={() => setOpen(false)}>Cancel</Button><Button type="button" onClick={save} disabled={saving}>{saving ? 'Saving…' : 'Save settings'}</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
