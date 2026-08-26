import { act, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TICK_MS, UsageMeter } from './UsageMeter'
import type { Usage } from '../../types'

const NOW = 1_787_745_600_000

function usage(over: Partial<Usage> = {}): Usage {
  return {
    percent: 42,
    resetsAtMs: NOW + 2 * 3_600_000 + 41 * 60_000,
    severity: 'normal',
    ...over,
  }
}

describe('UsageMeter', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(NOW)
  })

  afterEach(() => vi.useRealTimers())

  it('fills the bar to the share of the window that is spent', () => {
    render(<UsageMeter usage={usage({ percent: 42 })} />)

    const fill = screen.getByTestId('usage').querySelector<HTMLElement>('.usage-fill')
    expect(fill?.style.width).toBe('42%')
  })

  it('counts down to the reset rather than restating the percentage', () => {
    // The percentage is on the bar and in the tooltip; the time left is the
    // part worth spending glyphs on.
    render(<UsageMeter usage={usage()} />)

    expect(screen.getByTestId('usage')).toHaveTextContent('2h41m')
    expect(screen.getByTestId('usage')).toHaveAttribute(
      'title',
      '42% of the 5h limit used',
    )
  })

  it('carries severity so the bar can change colour without new markup', () => {
    render(<UsageMeter usage={usage({ severity: 'critical' })} />)

    expect(screen.getByTestId('usage')).toHaveAttribute('data-severity', 'critical')
  })

  it('advances the countdown as time passes, without new props', () => {
    render(<UsageMeter usage={usage({ resetsAtMs: NOW + 62 * 60_000 })} />)
    expect(screen.getByTestId('usage')).toHaveTextContent('1h02m')

    act(() => {
      vi.advanceTimersByTime(3 * 60_000)
    })

    expect(screen.getByTestId('usage')).toHaveTextContent('59m')
  })

  it('ticks slowly enough not to re-render the pill for an unchanged glyph', () => {
    // The label is floored to whole minutes, so a per-second tick would be
    // sixty renders a minute for nothing.
    expect(TICK_MS).toBeGreaterThanOrEqual(10_000)
  })

  it('stops ticking once unmounted', () => {
    const { unmount } = render(<UsageMeter usage={usage()} />)
    unmount()

    // A leaked interval would keep calling setState on a gone component.
    expect(vi.getTimerCount()).toBe(0)
  })
})
