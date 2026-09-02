import { describe, expect, it } from 'vitest'
import type { Alert, SessionSnapshot, Task, Usage } from '../../types'
import { deriveHeat, isCalm, JITTER_FULL_MS, JITTER_START_MS } from './heat'

function session(over: Partial<SessionSnapshot>): SessionSnapshot {
  return {
    pid: 1,
    sessionId: 's',
    name: 'repo',
    title: null,
    cwd: '/tmp',
    entrypoint: 'cli',
    state: 'idle',
    detail: null,
    elapsedMs: 0,
    uptimeMs: 0,
    statusTimeMs: 0,
    startedAtMs: 0,
    background: false,
    tasks: [],
    ...over,
  }
}

function usage(over: Partial<Usage>): Usage {
  return { percent: 10, resetsAtMs: 0, severity: 'normal', ...over }
}

function died(sessionId: string): Alert {
  return { sessionId, name: 'repo', kind: 'died', detail: null }
}

const shellTask = (): Task => ({
  id: 't1',
  kind: 'shell',
  label: 'npm test',
  startedAtMs: 0,
  endedAtMs: null,
  status: 'running',
})

const jobTask = (): Task => ({ ...shellTask(), id: 'job_1', kind: 'job' })

describe('fire', () => {
  it('counts busy sessions', () => {
    const sessions = [
      session({ sessionId: 'a', state: 'busy' }),
      session({ sessionId: 'b', state: 'busy' }),
      session({ sessionId: 'c', state: 'idle' }),
    ]
    expect(deriveHeat(sessions, null, []).fire).toBe(2)
  })

  it('caps at three however many are working', () => {
    const sessions = Array.from({ length: 7 }, (_, i) =>
      session({ sessionId: `s${i}`, state: 'busy' }),
    )
    expect(deriveHeat(sessions, null, []).fire).toBe(3)
  })

  it('ignores background jobs entirely', () => {
    const sessions = [
      session({ sessionId: 'a', state: 'busy', background: true }),
      session({ sessionId: 'b', state: 'busy', background: true }),
      session({ sessionId: 'c', state: 'busy' }),
    ]
    expect(deriveHeat(sessions, null, []).fire).toBe(1)
  })

  it('is zero when only background jobs are working', () => {
    const sessions = [session({ sessionId: 'a', state: 'busy', background: true })]
    expect(deriveHeat(sessions, null, []).fire).toBe(0)
  })

  it('counts a tasking session towards the fire', () => {
    const heat = deriveHeat([session({ state: 'tasking', tasks: [shellTask()] })], null, [])
    expect(heat.fire).toBe(1)
  })

  it('does not count a session whose only task is a registry job', () => {
    // Background jobs are already excluded from heat because they are work you
    // did not start. Counting the parent instead would be the same mistake in
    // a louder voice.
    const heat = deriveHeat([session({ state: 'tasking', tasks: [jobTask()] })], null, [])
    expect(heat.fire).toBe(0)
  })

  it('caps the fire at three across busy and tasking together', () => {
    const heat = deriveHeat(
      [
        session({ state: 'busy' }),
        session({ state: 'busy' }),
        session({ state: 'tasking', tasks: [shellTask()] }),
        session({ state: 'tasking', tasks: [shellTask()] }),
      ],
      null,
      [],
    )
    expect(heat.fire).toBe(3)
  })
})

describe('jitter', () => {
  it('is zero below the threshold', () => {
    const waiting = session({ state: 'waiting', elapsedMs: JITTER_START_MS - 1 })
    expect(deriveHeat([waiting], null, []).jitter).toBe(0)
  })

  it('is one at and beyond five minutes', () => {
    const at = session({ state: 'waiting', elapsedMs: JITTER_FULL_MS })
    const beyond = session({ state: 'waiting', elapsedMs: JITTER_FULL_MS * 4 })
    expect(deriveHeat([at], null, []).jitter).toBe(1)
    expect(deriveHeat([beyond], null, []).jitter).toBe(1)
  })

  it('ramps linearly between the two', () => {
    const half = JITTER_START_MS + (JITTER_FULL_MS - JITTER_START_MS) / 2
    const waiting = session({ state: 'waiting', elapsedMs: half })
    expect(deriveHeat([waiting], null, []).jitter).toBeCloseTo(0.5, 5)
  })

  it('reads the longest wait, not the first', () => {
    const sessions = [
      session({ sessionId: 'a', state: 'waiting', elapsedMs: JITTER_START_MS }),
      session({ sessionId: 'b', state: 'waiting', elapsedMs: JITTER_FULL_MS }),
    ]
    expect(deriveHeat(sessions, null, []).jitter).toBe(1)
  })

  it('ignores sessions that are not waiting', () => {
    const busy = session({ state: 'busy', elapsedMs: JITTER_FULL_MS })
    expect(deriveHeat([busy], null, []).jitter).toBe(0)
  })

  it('ignores background jobs', () => {
    const job = session({ state: 'waiting', elapsedMs: JITTER_FULL_MS, background: true })
    expect(deriveHeat([job], null, []).jitter).toBe(0)
  })
})

describe('strain', () => {
  it('maps each severity', () => {
    expect(deriveHeat([], usage({ severity: 'normal' }), []).strain).toBe(0)
    expect(deriveHeat([], usage({ severity: 'warn' }), []).strain).toBe(1)
    expect(deriveHeat([], usage({ severity: 'critical' }), []).strain).toBe(2)
  })

  it('is zero when there is no usage to read', () => {
    expect(deriveHeat([], null, []).strain).toBe(0)
  })
})

describe('ash', () => {
  it('lists the sessions that died in this update', () => {
    expect(deriveHeat([], null, [died('a'), died('b')]).ash).toEqual(['a', 'b'])
  })

  it('ignores alerts of other kinds', () => {
    const alerts: Alert[] = [
      { sessionId: 'a', name: 'repo', kind: 'needsInput', detail: null },
      { sessionId: 'b', name: 'repo', kind: 'finished', detail: null },
    ]
    expect(deriveHeat([], null, alerts).ash).toEqual([])
  })

  it('is empty for a session that is merely dead without a fresh alert', () => {
    const dead = session({ sessionId: 'a', state: 'dead' })
    expect(deriveHeat([dead], null, []).ash).toEqual([])
  })
})

describe('isCalm', () => {
  it('is true when nothing is happening', () => {
    expect(isCalm(deriveHeat([session({})], null, []))).toBe(true)
  })

  it('is false as soon as anything is', () => {
    expect(isCalm(deriveHeat([session({ state: 'busy' })], null, []))).toBe(false)
    expect(isCalm(deriveHeat([], usage({ severity: 'warn' }), []))).toBe(false)
    expect(isCalm(deriveHeat([], null, [died('a')]))).toBe(false)
  })
})
