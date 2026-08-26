import type { ReactNode } from 'react'
import './notchFlanks.css'

export type FlankSide = 'left' | 'right'

interface Props {
  side: FlankSide
  /** Slid under the notch and out of sight. */
  retracted: boolean
  /**
   * The notch's width. The chip slides by exactly its own width, so capping it
   * here is what guarantees it ends up entirely behind the notch rather than
   * peeking out the far side.
   */
  maxWidth: number
  chipRef?: React.Ref<HTMLDivElement>
  testId: string
  children: ReactNode
}

/**
 * One black box in the menu bar, flush against its edge of the notch.
 *
 * It never changes size. On hover it slides sideways into the notch's footprint
 * and is occluded by the bridge drawn over the notch, so the menu bar ends up
 * emptier while the panel is open than it is at rest — the opposite of the
 * earlier design, which grew outward over the app's menu titles.
 *
 * The slide is `translateX(100%)`, which is the chip's own width, so nothing has
 * to be measured for it.
 */
export function FlankChip({ side, retracted, maxWidth, chipRef, testId, children }: Props) {
  return (
    <div
      ref={chipRef}
      className="flank-chip"
      data-side={side}
      data-retracted={retracted ? 'true' : 'false'}
      data-testid={testId}
      style={{ maxWidth }}
    >
      {children}
    </div>
  )
}
