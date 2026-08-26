import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

/**
 * Where the chips go, in window-local px. Mirrors `notch::NotchLayout`.
 *
 * The page is told about the notch's edges and nothing about the screen. Global
 * coordinates stay in Rust, so there is exactly one place that decides where the
 * window sits and the page cannot disagree with it.
 */
export interface NotchLayout {
  /** Window-local x where the notch begins; the left chip's right edge. */
  notchLeft: number
  /** Window-local x where the notch ends; the right chip's left edge. */
  notchRight: number
  barHeight: number
  budget: number
  /** Width of the slab the widget becomes while open. */
  slabWidth: number
}

/** Rust re-probed the display configuration; mirrors `notch::NOTCH_EVENT`. */
export const NOTCH_EVENT = 'notch://layout'

export interface HoverRect {
  x: number
  y: number
  width: number
  height: number
}

/**
 * Report the widget's own boxes as the hover target.
 *
 * A list rather than one box because the caller decides: notch mode reports the
 * black band, which is one box spanning the notch, and an earlier design that
 * flanked the notch with two chips reported those.
 */
export function reportHoverRects(rects: HoverRect[]): void {
  void invoke('set_hover_rects', { rects }).catch(() => {
    // Falls back to whatever was last reported, which is stale rather than
    // broken. Hover must not be able to take the widget down.
  })
}

/**
 * Convert measured boxes into hover rects, dropping the ones that are not there.
 *
 * A side with no sessions renders no chip, and the popover is absent unless an
 * entry is hovered, so two of the three inputs are routinely missing. A
 * zero-sized box is dropped too: an element that exists but has not been laid
 * out yet would otherwise report a degenerate rect at the window's origin.
 */
export function visibleRects(
  boxes: Array<{ left: number; top: number; width: number; height: number } | null | undefined>,
): HoverRect[] {
  const present = boxes.filter(
    (box): box is { left: number; top: number; width: number; height: number } =>
      box != null && box.width > 0 && box.height > 0,
  )
  return present.map((box) => ({
    x: box.left,
    y: box.top,
    width: box.width,
    height: box.height,
  }))
}

/**
 * Chip placement from Rust, or `null` where this display has no notch.
 *
 * Read once on mount and again whenever Rust re-probes, which it does when the
 * display configuration changes. Closing the lid takes the notched display away
 * entirely, and the widget has to stop drawing chips for a screen that is gone.
 */
export function useNotchLayout(): NotchLayout | null {
  const [layout, setLayout] = useState<NotchLayout | null>(null)

  useEffect(() => {
    let disposed = false
    let stop: (() => void) | undefined

    const read = () => {
      invoke<NotchLayout | null>('notch_layout')
        .then((next) => {
          if (!disposed) setLayout(next ?? null)
        })
        .catch(() => {
          // No notch is the safe reading: the caller renders nothing rather
          // than placing chips against a geometry it does not have.
          if (!disposed) setLayout(null)
        })
    }

    read()
    listen(NOTCH_EVENT, read).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })

    return () => {
      disposed = true
      stop?.()
    }
  }, [])

  return layout
}
