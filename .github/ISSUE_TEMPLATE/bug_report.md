---
name: Bug report
about: Something the widget does that it should not, or does not do that it should
title: ''
labels: bug
assignees: ''
---

<!--
The questions below are the ones that would otherwise be asked in the first
reply. Most of this widget's behaviour depends on the machine it is running on —
whether the display has a notch, which Claude Code entrypoint the session used,
what the settings say — so a report with the environment filled in can usually
be diagnosed without a round trip.

Please do not paste transcript content. The widget reads session transcripts,
so a bug about what it displays is tempting to illustrate with the text it read,
and that text is whatever you were working on. Describe the shape of it instead
— "a question about 40 characters long", "a tool name with a slash in it".
-->

## What happened

<!-- One or two sentences. -->

## Expected

## Actual

## Steps to reproduce

1.
2.
3.

<!-- If it only happens sometimes, say roughly how often, and what was going on
     the times it did. -->

## Screenshot

<!-- This is a visual widget, and a screenshot of it in the wrong state is worth
     more than any description of that state. Attach one if the bug is anything
     you can see. A screen recording is better still for hover, animation or
     anything that only happens in motion. -->

## Environment

- **claude-buddy version:**
- **Installed by:** <!-- built locally with `npm run tauri build`, or the DMG -->
- **macOS version:**
- **Mac model and chip:** <!-- e.g. MacBook Pro 14", M3 Pro — or Intel -->
- **Built-in display has a notch:** yes / no
- **Notch mode on:** yes / no <!-- Settings → Sit in the menu bar beside the notch -->
- **Multiple displays:** yes / no <!-- if yes, which one is the widget on -->

## The session involved

<!-- Skip this if the bug is not about a particular session. -->

- **Entrypoint:** `cli` / `claude-desktop` / `sdk`
  <!-- The popover shows this. It matters: only `cli` sessions report a status,
       `claude-desktop` sessions are inferred from transcript timestamps, and
       `sdk` entries are never shown at all. -->
- **State the widget showed:** waiting / working / idle / paused / died
- **State it should have shown:**
- **Background jobs or subagents involved:** yes / no

## Settings

<!-- Paste `~/Library/Application Support/com.claude.buddy/config.json`. It
     carries no session data — just the switches and remembered positions —
     and it is usually the fastest way to see which of them is in play. -->

```json

```

## Anything else

<!-- Console output, whether it started after an upgrade, whether restarting
     the app clears it. -->
