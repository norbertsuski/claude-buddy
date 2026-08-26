import type { ReactNode } from 'react'
import './notchFlanks.css'

export type FlankSide = 'left' | 'right'

interface Props {
  side: FlankSide
  chipRef?: React.Ref<HTMLDivElement>
  testId: string
  children: ReactNode
}

/**
 * One black box in the menu bar, flush against its edge of the notch.
 *
 * It never changes size or moves. The slab that opens on hover is wider than
 * both chips and is painted over them, so nothing has to slide out of the way —
 * an earlier design retracted them into the notch, which is no longer needed
 * now that the black stays in the bar.
 */
export function FlankChip({ side, chipRef, testId, children }: Props) {
  return (
    <div
      ref={chipRef}
      className="flank-chip"
      data-side={side}
      data-testid={testId}
    >
      {children}
    </div>
  )
}
