import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { NamedDotRow } from './NamedDotRow'
import type { SessionSnapshot, SessionState } from '../../types'

function session(name: string, state: SessionState): SessionSnapshot {
  return {
    pid: 1,
    sessionId: `id-${name}`,
    name,
    cwd: `/Users/n/Code/${name}`,
    entrypoint: 'cli',
    state,
    detail: state === 'waiting' ? 'input needed' : null,
    elapsedMs: 60_000,
    uptimeMs: 60_000,
    background: false,
  }
}

describe('NamedDotRow', () => {
  it('renders one entry per session with the suffix stripped', () => {
    render(
      <NamedDotRow
        sessions={[session('api-service-55', 'waiting'), session('web-app-e2', 'busy')]}
        hoveredSessionId={null}
        onHoverSession={vi.fn()}
      />,
    )
    expect(screen.getByText('api-service')).toBeInTheDocument()
    expect(screen.getByText('web-app')).toBeInTheDocument()
  })

  it('marks each entry with its state for styling', () => {
    render(
      <NamedDotRow
        sessions={[session('a-11', 'waiting')]}
        hoveredSessionId={null}
        onHoverSession={vi.fn()}
      />,
    )
    expect(screen.getByTestId('session-id-a-11')).toHaveAttribute('data-state', 'waiting')
  })

  it('reports the hovered session', async () => {
    const onHover = vi.fn()
    render(
      <NamedDotRow
        sessions={[session('a-11', 'busy')]}
        hoveredSessionId={null}
        onHoverSession={onHover}
      />,
    )

    await userEvent.hover(screen.getByTestId('session-id-a-11'))

    expect(onHover).toHaveBeenCalledWith('id-a-11')
  })

  it('reports null when the cursor leaves an entry', async () => {
    const onHover = vi.fn()
    render(
      <NamedDotRow
        sessions={[session('a-11', 'busy')]}
        hoveredSessionId="id-a-11"
        onHoverSession={onHover}
      />,
    )

    await userEvent.unhover(screen.getByTestId('session-id-a-11'))

    expect(onHover).toHaveBeenLastCalledWith(null)
  })

  it('flags the hovered entry so it can be highlighted', () => {
    render(
      <NamedDotRow
        sessions={[session('a-11', 'busy')]}
        hoveredSessionId="id-a-11"
        onHoverSession={vi.fn()}
      />,
    )
    expect(screen.getByTestId('session-id-a-11')).toHaveAttribute('data-hovered', 'true')
  })

  it('caps the row and reports the overflow count', () => {
    const many = Array.from({ length: 11 }, (_, i) => session(`proj${i}-11`, 'busy'))
    render(<NamedDotRow sessions={many} hoveredSessionId={null} onHoverSession={vi.fn()} />)

    expect(screen.getAllByTestId(/^session-/)).toHaveLength(8)
    expect(screen.getByTestId('overflow')).toHaveTextContent('+3 more')
  })

  it('shows no overflow marker at exactly the cap', () => {
    const many = Array.from({ length: 8 }, (_, i) => session(`proj${i}-11`, 'busy'))
    render(<NamedDotRow sessions={many} hoveredSessionId={null} onHoverSession={vi.fn()} />)

    expect(screen.queryByTestId('overflow')).not.toBeInTheDocument()
  })
})


describe('NamedDotRow background jobs', () => {
  const job = (name: string): SessionSnapshot => ({
    ...session(name, 'busy'),
    background: true,
  })

  it('separates a job from its parent with an arrow, not a divider', () => {
    const { container } = render(
      <NamedDotRow
        sessions={[session('api-service-7a', 'busy'), job('migrate-schemas')]}
        hoveredSessionId={null}
        onHoverSession={vi.fn()}
      />,
    )
    expect(container.querySelector('.child-arrow')).not.toBeNull()
    expect(container.querySelectorAll('.hairline')).toHaveLength(0)
  })

  it('still divides two sessions with a hairline', () => {
    const { container } = render(
      <NamedDotRow
        sessions={[session('a-11', 'busy'), session('b-22', 'idle')]}
        hoveredSessionId={null}
        onHoverSession={vi.fn()}
      />,
    )
    expect(container.querySelectorAll('.hairline')).toHaveLength(1)
    expect(container.querySelector('.child-arrow')).toBeNull()
  })

  it('marks jobs so they can be demoted visually', () => {
    render(
      <NamedDotRow
        sessions={[session('a-11', 'busy'), job('migrate-schemas')]}
        hoveredSessionId={null}
        onHoverSession={vi.fn()}
      />,
    )
    expect(screen.getByTestId('session-id-migrate-schemas')).toHaveAttribute(
      'data-background',
      'true',
    )
  })
})
