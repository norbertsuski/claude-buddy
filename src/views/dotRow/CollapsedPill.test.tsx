import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { CollapsedPill } from './CollapsedPill'
import type { SessionSnapshot, SessionState } from '../../types'

function session(id: string, state: SessionState): SessionSnapshot {
  return {
    pid: 1,
    sessionId: id,
    name: `${id}-a1`,
    title: null,
    cwd: `/Users/n/Code/${id}`,
    entrypoint: 'cli',
    state,
    detail: state === 'waiting' ? 'input needed' : null,
    elapsedMs: 0,
    uptimeMs: 0,
    statusTimeMs: 0,
    startedAtMs: 0,
    background: false,
    tasks: [],
  }
}

function job(id: string, state: SessionState): SessionSnapshot {
  return { ...session(id, state), background: true }
}

describe('CollapsedPill', () => {
  it('counts a waiting background job in the needs-you chip', () => {
    // Regression: a job blocked on input alerted and kept the widget on
    // screen, but the collapsed pill — the surface you actually rest your eyes
    // on — reported only "N jobs" and showed nothing amber.
    render(<CollapsedPill sessions={[session('a', 'idle'), job('j', 'waiting')]} />)
    expect(screen.getByTestId('needs-you')).toHaveTextContent('1 needs you')
  })

  it('counts a dead background job in the died chip', () => {
    render(<CollapsedPill sessions={[session('a', 'idle'), job('j', 'dead')]} />)
    expect(screen.getByTestId('died')).toHaveTextContent('1 died')
  })

  it('does not tally a surfaced job in the quiet jobs summary as well', () => {
    // Otherwise "1 needs you · 2 jobs" double-counts the same entry.
    render(
      <CollapsedPill sessions={[session('a', 'idle'), job('j', 'waiting'), job('k', 'idle')]} />,
    )
    expect(screen.getByTestId('needs-you')).toHaveTextContent('1 needs you')
    expect(screen.getByTestId('jobs')).toHaveTextContent('1 job')
  })

  it('still keeps ordinary jobs out of the working and idle counts', () => {
    render(<CollapsedPill sessions={[session('a', 'busy'), job('j', 'busy'), job('k', 'idle')]} />)
    expect(screen.getByTestId('working')).toHaveTextContent('1 working')
    expect(screen.getByTestId('jobs')).toHaveTextContent('2 jobs')
  })

  it('shows the needs-you chip with a count when sessions are waiting', () => {
    render(<CollapsedPill sessions={[session('a', 'waiting'), session('b', 'busy')]} />)
    expect(screen.getByTestId('needs-you')).toHaveTextContent('1 needs you')
  })

  it('shows working as its own chip, like waiting and died', () => {
    render(<CollapsedPill sessions={[session('a', 'busy'), session('b', 'busy')]} />)
    expect(screen.getByTestId('working')).toHaveTextContent('2 working')
  })

  it('shows waiting, working and died chips side by side', () => {
    render(
      <CollapsedPill
        sessions={[session('a', 'waiting'), session('b', 'busy'), session('c', 'dead')]}
      />,
    )
    expect(screen.getByTestId('needs-you')).toHaveTextContent('1 needs you')
    expect(screen.getByTestId('working')).toHaveTextContent('1 working')
    expect(screen.getByTestId('died')).toHaveTextContent('1 died')
  })

  it('omits the needs-you chip entirely when nothing is waiting', () => {
    render(<CollapsedPill sessions={[session('a', 'busy')]} />)
    expect(screen.queryByTestId('needs-you')).not.toBeInTheDocument()
  })

  it('counts waiting sessions across several at once', () => {
    render(<CollapsedPill sessions={[session('a', 'waiting'), session('b', 'waiting')]} />)
    expect(screen.getByTestId('needs-you')).toHaveTextContent('2 need you')
  })

  it('keeps the idle count as quiet text', () => {
    render(<CollapsedPill sessions={[session('a', 'idle'), session('b', 'paused')]} />)
    expect(screen.getByTestId('summary')).toHaveTextContent('2 idle')
    expect(screen.queryByTestId('working')).not.toBeInTheDocument()
  })

  it('reports dead sessions', () => {
    render(<CollapsedPill sessions={[session('a', 'dead')]} />)
    expect(screen.getByTestId('died')).toHaveTextContent('1 died')
  })

  it('shows a resting label when there are no sessions at all', () => {
    render(<CollapsedPill sessions={[]} />)
    expect(screen.getByTestId('summary')).toHaveTextContent('no sessions')
  })

  it('shows tasking as its own chip, between working and died', () => {
    render(
      <CollapsedPill
        sessions={[session('a', 'busy'), session('b', 'tasking'), session('c', 'dead')]}
      />,
    )
    expect(screen.getByTestId('tasking')).toHaveTextContent('1 on a task')
    const chips = screen.getAllByTestId(/^(needs-you|working|tasking|died)$/)
    expect(chips.map((c) => c.getAttribute('data-testid'))).toEqual([
      'working',
      'tasking',
      'died',
    ])
  })

  it('pluralises the tasking chip', () => {
    render(<CollapsedPill sessions={[session('a', 'tasking'), session('b', 'tasking')]} />)
    expect(screen.getByTestId('tasking')).toHaveTextContent('2 on tasks')
  })

  it('omits the tasking chip when nothing is tasking', () => {
    render(<CollapsedPill sessions={[session('a', 'idle')]} />)
    expect(screen.queryByTestId('tasking')).toBeNull()
  })

  it('does not let a tasking session go unmentioned', () => {
    // The gap this task closes: a tasking session used to contribute to no
    // chip and no summary, so the resting pill said nothing about it at all.
    render(<CollapsedPill sessions={[session('a', 'tasking')]} />)
    expect(screen.getByTestId('collapsed-pill').textContent).not.toBe('')
  })

  it('still keeps background jobs out of the tasking count', () => {
    // Regression: the tasking chip must read from counts (ownSessions only),
    // not urgent (all sessions including jobs), or a background job in tasking
    // state would double-count the entry as both a task and a job.
    render(<CollapsedPill sessions={[session('a', 'tasking'), job('j', 'tasking')]} />)
    expect(screen.getByTestId('tasking')).toHaveTextContent('1 on a task')
    expect(screen.getByTestId('jobs')).toHaveTextContent('1 job')
  })
})


describe('CollapsedPill background jobs', () => {
  const job = (id: string): SessionSnapshot => ({ ...session(id, 'busy'), background: true })

  it('does not let jobs inflate the session counts', () => {
    render(<CollapsedPill sessions={[session('a', 'busy'), job('j1'), job('j2')]} />)
    expect(screen.getByTestId('working')).toHaveTextContent('1 working')
  })

  it('acknowledges jobs separately', () => {
    render(<CollapsedPill sessions={[session('a', 'busy'), job('j1'), job('j2')]} />)
    expect(screen.getByTestId('jobs')).toHaveTextContent('2 jobs')
  })

  it('says nothing about jobs when there are none', () => {
    render(<CollapsedPill sessions={[session('a', 'busy')]} />)
    expect(screen.queryByTestId('jobs')).not.toBeInTheDocument()
  })
})

describe('CollapsedPill chip identity', () => {
  // The chips fade in as they appear, which is only right if a chip that was
  // already there is left alone. React matches `{cond && <chip/>}` children by
  // position, so a chip must survive both a count change beside it and a
  // different chip appearing before it — otherwise every tick of a count would
  // re-run the fade on everything in the row.
  it('keeps the same element when only its count changes', () => {
    const { rerender } = render(
      <CollapsedPill sessions={[session('a', 'busy'), session('b', 'idle')]} />,
    )
    const before = screen.getByTestId('working')

    rerender(<CollapsedPill sessions={[session('a', 'busy'), session('b', 'busy')]} />)

    expect(screen.getByTestId('working')).toBe(before)
    expect(screen.getByTestId('working')).toHaveTextContent('2 working')
  })

  it('keeps the working chip when a waiting chip appears before it', () => {
    const { rerender } = render(
      <CollapsedPill sessions={[session('a', 'busy'), session('b', 'idle')]} />,
    )
    const before = screen.getByTestId('working')

    rerender(<CollapsedPill sessions={[session('a', 'busy'), session('b', 'waiting')]} />)

    expect(screen.getByTestId('needs-you')).toBeInTheDocument()
    expect(screen.getByTestId('working')).toBe(before)
  })
})

describe('CollapsedPill usage meter', () => {
  const usage = { percent: 42, resetsAtMs: Date.now() + 3_600_000, severity: 'normal' as const }

  it('shows the meter at the end of the row, after the quiet summary', () => {
    render(<CollapsedPill sessions={[session('a', 'busy'), session('b', 'idle')]} usage={usage} />)

    const variant = screen.getByTestId('collapsed-pill')
    const children = Array.from(variant.children)
    expect(children[children.length - 1]).toBe(screen.getByTestId('usage'))
  })

  it('shows nothing when there is no figure worth trusting', () => {
    // Which is also how the setting being off arrives here: the two cases are
    // deliberately indistinguishable to the row.
    render(<CollapsedPill sessions={[session('a', 'busy')]} usage={null} />)

    expect(screen.queryByTestId('usage')).not.toBeInTheDocument()
  })

  it('shows nothing when no usage is passed at all', () => {
    render(<CollapsedPill sessions={[session('a', 'busy')]} />)

    expect(screen.queryByTestId('usage')).not.toBeInTheDocument()
  })
})
