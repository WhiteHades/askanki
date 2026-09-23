import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ApprovalCard, ThinkingState } from './beautiful-primitives'

describe('Beautiful UI state primitives', () => {
  it('exposes real activity steps and a stop action', () => {
    const onCancel = vi.fn()
    render(<ThinkingState steps={[{ id: 'read', label: 'Read card', status: 'running' }]} active onCancel={onCancel} />)

    fireEvent.click(screen.getByRole('button', { name: 'Stop' }))
    expect(screen.getByText('Read card')).toBeInTheDocument()
    expect(onCancel).toHaveBeenCalledOnce()
  })

  it('settles an approval decision', () => {
    const onDecision = vi.fn()
    render(<ApprovalCard summary="Write to the workspace" onDecision={onDecision} />)

    fireEvent.click(screen.getByRole('button', { name: 'Allow' }))
    expect(onDecision).toHaveBeenCalledWith(true)
    expect(screen.getByText('Approved')).toBeInTheDocument()
  })
})
