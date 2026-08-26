import { useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { layoutSize } from '../../useWidgetSize'
import './notchFlanks.css'

export type FlankSide = 'left' | 'right'

/** Chip padding and border, mirroring `.flank-chip` in notchFlanks.css. */
export const CHIP_PAD = 10
export const CHIP_BORDER = 0

interface Props {
  side: FlankSide
  /** Resting content. */
  collapsed: ReactNode
  /** Content once the cursor is on the widget. */
  expanded: ReactNode
  showExpanded: boolean
  /**
   * Set on the chip itself, not on the flank around it. The flank spans the
   * whole budget; reporting that as the hover target would make empty menu bar
   * beside the chip read as hovering the widget.
   */
  chipRef?: React.Ref<HTMLDivElement>
  testId: string
}

/**
 * The black box either side of the notch, and the morph between its two states.
 *
 * Both states are mounted at once so the one being morphed into can be measured
 * before the transition starts, and the chip's own width is animated between
 * them — the same approach `DotRow` takes for the pill, minus the window
 * choreography, because the notch window is a fixed size big enough for either
 * state and never resizes. `auto` cannot be transitioned, which is why the width
 * is written inline from the measurement.
 *
 * Shared by both flanks: the counts on the left and the five-hour limit on the
 * right are different content in an identical box, and a second copy of the
 * measuring drifted from the first the moment either changed.
 */
export function FlankChip({
  side,
  collapsed,
  expanded,
  showExpanded,
  chipRef,
  testId,
}: Props) {
  const collapsedSlot = useRef<HTMLDivElement>(null)
  const expandedSlot = useRef<HTMLDivElement>(null)
  const [boxWidth, setBoxWidth] = useState<number | null>(null)

  useLayoutEffect(() => {
    const slot = (showExpanded ? expandedSlot : collapsedSlot).current
    if (slot === null) return
    // layoutSize, not getBoundingClientRect: the hidden slot is mid-transition
    // and the rect would report its animating state rather than its layout.
    setBoxWidth(layoutSize(slot).width + CHIP_PAD * 2 + CHIP_BORDER * 2)
  }, [showExpanded, collapsed, expanded])

  return (
    <div
      ref={chipRef}
      className="flank-chip"
      data-side={side}
      data-expanded={showExpanded ? 'true' : 'false'}
      data-testid={testId}
      style={boxWidth === null ? undefined : { width: boxWidth }}
    >
      <div
        className="flank-variant"
        ref={collapsedSlot}
        data-show={showExpanded ? 'false' : 'true'}
        data-testid={`${testId}-collapsed`}
      >
        {collapsed}
      </div>
      <div
        className="flank-variant"
        ref={expandedSlot}
        data-show={showExpanded ? 'true' : 'false'}
        data-testid={`${testId}-expanded`}
      >
        {expanded}
      </div>
    </div>
  )
}
