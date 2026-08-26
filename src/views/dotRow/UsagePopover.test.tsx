import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { UsagePopover } from './UsagePopover'
import type { Usage } from '../../types'

const NOW = 1_787_745_600_000

function usage(over: Partial<Usage> = {}): Usage {
  return { percent: 42, resetsAtMs: NOW + 2 * 3_600_000 + 41 * 60_000, severity: 'normal', ...over }
}

describe('UsagePopover', () => {
  it('gives the percentage the meter has no room to print', () => {
    render(<UsagePopover usage={usage()} now={NOW} />)

    expect(screen.getByTestId('usage-popover-percent')).toHaveTextContent('42%')
  })

  it('gives the reset both as a countdown and as a wall-clock time', () => {
    // The countdown answers "how long have I got"; the clock time answers
    // "can I start this before dinner", and neither substitutes for the other.
    render(<UsagePopover usage={usage()} now={NOW} />)

    const resets = screen.getByTestId('usage-popover-resets')
    expect(resets).toHaveTextContent('in 2h41m')
    expect(resets).toHaveTextContent(
      new Date(NOW + 2 * 3_600_000 + 41 * 60_000).toLocaleTimeString(undefined, {
        hour: '2-digit',
        minute: '2-digit',
      }),
    )
  })

  it('marks a spent window hot', () => {
    render(<UsagePopover usage={usage({ severity: 'critical', percent: 97 })} now={NOW} />)

    expect(screen.getByTestId('usage-popover-percent')).toHaveClass('hot')
  })

  it('says where the figure comes from, since it can be behind', () => {
    render(<UsagePopover usage={usage()} now={NOW} />)

    expect(screen.getByTestId('usage-popover')).toHaveTextContent('usage cache')
  })
})
