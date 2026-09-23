export type RuntimeConfig = {
  provider: 'opencode' | 'codex'
  workspace: string
  auto_fallback: boolean
  history_retention_days: number
  system_instruction: string
}

export type HistoryEntry = {
  id: string
  note_id: string
  role: 'user' | 'assistant' | 'tool' | 'error' | 'system'
  content: string
  created_at: number
}

export type CardContext = {
  noteId: string | null
  text: string
  imageCount: number
  hasImages: boolean
  hasMath: boolean
  hasCode: boolean
}

export type ChatMessage = {
  id: string
  role: 'user' | 'assistant' | 'error'
  content: string
  createdAt: number
  streaming?: boolean
}

export type RunStep = {
  id: string
  label: string
  detail?: string
  status: 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled'
}

export type RunState = 'idle' | 'starting' | 'running' | 'waiting-approval' | 'stopping'

export type AgentRunResult = {
  text: string
  steps?: RunStep[]
  tools?: Array<{ id: string; label: string; detail?: string; status: RunStep['status'] }>
}
