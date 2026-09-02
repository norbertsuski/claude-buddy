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
    title: null,
    cwd: `/Users/n/Code/${name}`,
    entrypoint: 'cli',
    state,
    detail: state === 'waiting' ? 'input needed' : null,
    elapsedMs: 60_000,
    uptimeMs: 60_000,
    statusTimeMs: 0,
    startedAtMs: 0,
    background: false,
    tasks: [],
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

  it('shows the session title in place of the folder name', () => {
    render(
      <NamedDotRow
        sessions={[
          { ...session('api-service-55', 'waiting'), title: 'Rate limit bucket key' },
          session('web-app-e2', 'busy'),
        ]}
        hoveredSessionId={null}
        onHoverSession={vi.fn()}
      />,
    )
    expect(screen.getByText('Rate limit bucket key')).toBeInTheDocument()
    expect(screen.queryByText('api-service')).not.toBeInTheDocument()
    // Untitled sessions keep the folder name they have always had.
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

  it('gives every state its own dot class so shape can differ, not just hue', () => {
    const states: SessionState[] = ['waiting', 'busy', 'idle', 'paused', 'dead']
    const sessions = states.map((state, i) => session(`project${i}-11`, state))

    const { container } = render(
      <NamedDotRow sessions={sessions} hoveredSessionId={null} onHoverSession={vi.fn()} />,
    )

    for (const state of states) {
      expect(container.querySelector(`.dot-${state}`)).not.toBeNull()
    }
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

describe('NamedDotRow usage meter', () => {
  const usage = { percent: 42, resetsAtMs: Date.now() + 3_600_000, severity: 'normal' as const }
  const sessions = [session('api-service', 'busy'), session('web-app', 'idle')]

  it('keeps the meter on screen while the row is expanded', () => {
    // Hovering must not make it vanish: the popover only opens over a name, so
    // between two of them there would otherwise be nowhere showing it.
    render(
      <NamedDotRow
        sessions={sessions}
        hoveredSessionId={null}
        onHoverSession={vi.fn()}
        usage={usage}
      />,
    )

    expect(screen.getByTestId('usage')).toBeInTheDocument()
  })

  it('puts it last, after the overflow count', () => {
    render(
      <NamedDotRow
        sessions={sessions}
        hoveredSessionId={null}
        onHoverSession={vi.fn()}
        usage={usage}
      />,
    )

    const row = screen.getByTestId('named-dot-row')
    expect(row.children[row.children.length - 1]).toBe(screen.getByTestId('usage'))
  })

  it('shows nothing when there is no figure worth trusting', () => {
    render(
      <NamedDotRow sessions={sessions} hoveredSessionId={null} onHoverSession={vi.fn()} />,
    )

    expect(screen.queryByTestId('usage')).not.toBeInTheDocument()
  })
})
