import { describe, expect, it } from 'vitest'
import { formatCountdown, formatElapsed } from './format'

describe('formatElapsed', () => {
  it('renders whole minutes under an hour', () => {
    expect(formatElapsed(6 * 60 * 1000)).toBe('6m')
  })

  it('renders seconds under a minute', () => {
    expect(formatElapsed(42 * 1000)).toBe('42s')
  })

  it('renders hours and minutes past an hour', () => {
    expect(formatElapsed(6 * 60 * 60 * 1000 + 55 * 60 * 1000)).toBe('6h55m')
  })

  it('clamps negative input to zero', () => {
    expect(formatElapsed(-5000)).toBe('0s')
  })
})

import { countByState, rowLabel, ROW_LABEL_MAX_CHARS, shortName } from './format'
import type { SessionSnapshot, SessionState } from './types'

function s(state: SessionState): SessionSnapshot {
  return {
    pid: 1,
    sessionId: `id-${state}-${Math.random()}`,
    name: 'proj-a1',
    title: null,
    cwd: '/Users/n/Code/proj',
    entrypoint: 'cli',
    state,
    detail: null,
    elapsedMs: 0,
    uptimeMs: 0,
    statusTimeMs: 0,
    startedAtMs: 0,
    background: false,
    tasks: [],
  }
}

describe('shortName', () => {
  it('strips the two-character suffix Claude Code appends', () => {
    expect(shortName('api-service-55')).toBe('api-service')
    expect(shortName('claude-buddy-1f')).toBe('claude-buddy')
    expect(shortName('web-app-e2')).toBe('web-app')
  })

  it('leaves a name without that suffix alone', () => {
    expect(shortName('api-service')).toBe('api-service')
  })

  it('does not strip a longer trailing segment', () => {
    expect(shortName('my-app-staging')).toBe('my-app-staging')
  })

  it('never returns an empty string', () => {
    expect(shortName('a1')).toBe('a1')
    expect(shortName('')).toBe('')
  })
})

describe('rowLabel', () => {
  const titled = (title: string | null): SessionSnapshot => ({
    ...s('idle'),
    name: 'api-service-55',
    title,
  })

  it('prefers the session title', () => {
    expect(rowLabel(titled('Rate limit bucket key'))).toBe('Rate limit bucket key')
  })

  it('falls back to the shortened registry name when there is no title', () => {
    expect(rowLabel(titled(null))).toBe('api-service')
  })

  it('treats a blank title as no title', () => {
    expect(rowLabel(titled('   '))).toBe('api-service')
  })

  it('clips a long title rather than widening the pill', () => {
    const label = rowLabel(titled('Rewrite the whole installation guide from scratch'))
    expect(label).toBe('Rewrite the whole inst\u2026')
    expect(label.length).toBeLessThanOrEqual(ROW_LABEL_MAX_CHARS + 1)
  })
})

describe('countByState', () => {
  it('counts each state', () => {
    const counts = countByState([s('waiting'), s('busy'), s('busy'), s('paused')])
    expect(counts.waiting).toBe(1)
    expect(counts.busy).toBe(2)
    expect(counts.paused).toBe(1)
    expect(counts.idle).toBe(0)
    expect(counts.dead).toBe(0)
  })

  it('returns all zeroes for no sessions', () => {
    const counts = countByState([])
    expect(Object.values(counts).every((n) => n === 0)).toBe(true)
  })

  it('counts tasking sessions', () => {
    const counts = countByState([s('tasking'), s('tasking'), s('idle')])
    expect(counts.tasking).toBe(2)
    expect(counts.idle).toBe(1)
  })
})

describe('formatCountdown', () => {
  it('reads in hours and minutes for most of a five-hour window', () => {
    expect(formatCountdown(2 * 3_600_000 + 41 * 60_000)).toBe('2h41m')
  })

  it('pads the minutes so the width does not change as they tick down', () => {
    // The meter sits at the end of the row; a glyph appearing or vanishing
    // there would change the pill's width and re-run the whole morph.
    expect(formatCountdown(3 * 3_600_000 + 7 * 60_000)).toBe('3h07m')
    expect(formatCountdown(3 * 3_600_000)).toBe('3h00m')
  })

  it('drops the hours once there are none', () => {
    expect(formatCountdown(59 * 60_000)).toBe('59m')
    expect(formatCountdown(60_000)).toBe('1m')
  })

  it('never counts seconds, and never counts below zero', () => {
    expect(formatCountdown(59_000)).toBe('<1m')
    expect(formatCountdown(0)).toBe('<1m')
    expect(formatCountdown(-5_000)).toBe('<1m')
  })
})
