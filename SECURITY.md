# Security policy

claude-buddy sits next to Claude Code and reads the things Claude Code leaves on
disk: the session registry, the session transcripts, and the OAuth token Claude
Code holds. That is a genuinely sensitive neighbourhood to be standing in, so
this page says exactly what the app touches, what counts as a vulnerability
here, and how to tell someone about one without telling everyone at once.

## Which versions get fixes

The latest release, and only the latest release.

The project is at 0.4.0 and pre-1.0, and there are no maintenance branches
behind it. There is no supported-version table to consult because there is no
second supported version: a fix lands on `main` and goes out in the next tag.
If you are running something older, the answer to "is this fixed in my version"
is going to be "upgrade", so the honest thing is to say so here rather than
imply a support window that does not exist.

| Version | Supported |
|---|---|
| 0.5.x (latest release) | yes |
| anything earlier | no — upgrade |

## Reporting a vulnerability

**Please do not open a public issue for a security problem**, and please do not
open a pull request that fixes one, since the diff describes the bug to
everyone watching the project before anyone has a build that closes it.

Use GitHub's **private vulnerability reporting** instead. It is a form attached
to the repository itself: what you write is visible to you and the maintainer
and to nobody else, the thread that follows stays private for as long as it
needs to, and the draft advisory it opens is the thing that eventually gets
published once a fix is out — so the report, the conversation and the public
record are one object rather than three.

To open one:

1. Go to the repository's
   [Security tab](https://github.com/norbertsuski/claude-buddy/security).
2. Press **Report a vulnerability**.
3. Write the report and submit it. Nothing is public at any point in that
   sequence, so there is no moment to get wrong.

One caveat, and it is the maintainer's to fix rather than yours: private
vulnerability reporting is a per-repository setting — *Settings → Code security
→ Private vulnerability reporting* — and the button only exists once it has been
switched on. If it is not there, open an ordinary issue saying you have a
security report and nothing else about it, and the maintainer can enable the
feature and pick it up from there. An issue that says "I have found something"
tells an attacker nothing they can use.

<!-- TODO(maintainer): add a private contact address here if you want one.
     Private vulnerability reporting needs a GitHub account and is enough on its
     own; an email line is only worth adding if you are happy to publish an
     address, and it is the obvious fallback while the feature is switched off. -->
<!-- Optional private contact: <address here> -->

### What to include

Enough to reproduce it. The version, the macOS version, what the app did, and
what it should have done instead. If it involves the token or a transcript, say
which of the three token sources was in play (see below) rather than pasting
anything you read out of them — a report should never carry a real token, and
it should never carry transcript content, which is whatever you happened to be
working on at the time.

### What to expect

This is a side project with one maintainer, so the timings below are what is
realistic rather than what sounds impressive:

- An acknowledgement within about a week. If a fortnight goes by with nothing,
  assume the notification was missed and post in the advisory thread again.
- An assessment — whether it reproduces, and whether it is a vulnerability or a
  bug — in the same thread, in the open with you.
- A fix in the next release if it is a real one, and the advisory published once
  that release is out, so the fix has a public record.
- Credit in [CHANGELOG.md](CHANGELOG.md) under the release that fixes it, unless
  you would rather not be named. Say which you prefer.

There is no bug bounty, and no money. Nobody is being paid to write this either.

## The sensitive surface

What follows is the part of the app worth attacking, described accurately. It
is also the part where a report is most likely to be about something the app
does on purpose, so it is worth reading before writing one.

### A borrowed OAuth token

The five-hour limit meter makes the same request Claude Code makes,
`GET https://api.anthropic.com/api/oauth/usage`, and it needs Claude Code's own
OAuth token to make it. The token is read from `CLAUDE_CODE_OAUTH_TOKEN`, then
`~/.claude/.credentials.json`, then the login Keychain, in that order, and used
as a bearer token for that one request and nothing else.

The word for the arrangement is *borrowed*. The app never refreshes the token,
never rotates it and never writes it anywhere — not to its own config, not to a
cache, not to a log. An access token whose `expiresAt` has passed is treated as
no token at all: the poll is skipped, and the next one five minutes later picks
up whatever Claude Code has refreshed to by then. Running the refresh flow would
mean rotating a credential out from under the process that owns it, which is not
this app's business.

The token is held in memory for the duration of a request. The Keychain read
shells out to `security find-generic-password -w`, deliberately, so the token
comes back on stdout and never appears in a process argument list — argument
lists are readable by every process on the machine, and a token in one is a
token published.

Turning the meter off with `showUsage` stops the requests, and with them the
token reads. That setting is checked once per poll rather than at startup, so it
takes effect without a restart.

### Read access to session transcripts

The watcher reads `~/.claude/sessions/<pid>.json` for the registry and the last
64 KiB of `~/.claude/projects/<slug>/<session-id>.jsonl` for the fields the
registry does not carry: the git branch, the model, the effort, the tool a
session most recently reached for, and the question a waiting session is
blocked on.

That last one is the one to think carefully about. **A transcript contains
whatever you were working on** — source, prose, paths, whatever was pasted into
the session — and the widget lifts prose out of it and puts it on screen. When
a session starts waiting, that prose also goes into a macOS notification, which
means it passes through Notification Center and can appear on a locked screen
if the system is configured to show notification content there. Nothing is sent
anywhere off the machine, but "on screen" and "in Notification Center" are a
wider audience than "in a terminal window you had to go and look at".

Reads only. claude-buddy is strictly read-only against `~/.claude` — it never
writes, moves or deletes anything under it. The only file the app writes at all
is its own `~/Library/Application Support/com.claude.buddy/config.json`. The one
other thing it reads outside `~/.claude` is the settings file left behind by the
pre-rename bundle identifier, once, to copy it across on first launch; that file
is read and never modified or removed.

### Keychain reads

The first Keychain read raises macOS's own permission dialog, from the system
rather than from the app. Denying it is a supported outcome and not a bug: no
token means no meter, and the rest of the widget carries on. The item read is
the generic password Claude Code files under the service `Claude Code-credentials`,
under the current user, and no other Keychain item is touched.

### Subprocesses

Three, all with fixed arguments: `security` for the Keychain read, `ps` for
process liveness and for walking the process tree, and `open -b <bundle-id>` to
raise a session's editor. `open` is used rather than AppleScript precisely
because it needs neither Accessibility nor Automation permission, so the app
never asks for either.

### The signed update channel

Updates are secured by a **minisign keypair, which is a separate thing from
Apple code signing** — it secures what the app will install, not what Gatekeeper
will launch. The updater verifies the signature on an update against the public
key in `tauri.conf.json` and refuses anything it cannot verify.

As shipped, `plugins.updater.pubkey` is empty and no private key is committed,
so the updater plugin is never registered: the launch check and the tray item
both return without making a network call, and the app stays on the version you
installed. A build that has a key checks once on launch and only notifies —
installing is always a deliberate menu item, never automatic, because replacing
a running menu-bar app under someone without asking is exactly the kind of
surprise this widget exists to avoid.

If you can get the updater to install something the key did not sign, that is a
vulnerability and a serious one. Report it privately, as above.

### What the frontend can invoke

The webview is not trusted with the machine. It can call exactly the commands
listed in the `invoke_handler` in `src-tauri/src/lib.rs` and nothing else, and
its capability grant in `src-tauri/capabilities/default.json` is the core event
API, window control, window close and the updater — no filesystem plugin, no
shell plugin, no HTTP plugin. Anything the page wants from disk it has to ask a
named Rust command for. A way to reach further than that list is worth
reporting.

## What is not a vulnerability here

Some of these get reported against projects like this one routinely, so they are
written down rather than left to a reply in a thread.

- **The app is unsigned and un-notarised.** Gatekeeper prompts once per install
  and you have to press *Open Anyway*. This is a documented limitation — getting
  rid of it needs an Apple Developer ID — and it is a known state of the project
  rather than a security report. See *Limitations* in [README.md](README.md).
  Update *delivery* is signed separately and does work; see above.
- **The app reads `~/.claude`.** That is the entire point of it, and it runs as
  you, with your permissions, reading files you already own. A report that the
  app can read your transcripts is a description of the feature. A report that
  it exposes them somewhere they should not go is not — send that one.
- **The updater does nothing on a stock build.** With no public key configured
  it never checks and never updates, by design.
- **The usage endpoint is private and undocumented.** It can change or vanish in
  any Claude Code release, and every step of the call is allowed to fail
  quietly. The meter disappearing is the intended failure mode, not a bug.
- **Scanner output with no reproduction.** A dependency-audit dump or a static
  analysis report with no path from an attacker to an effect is not something
  this project can act on. Show the path.

## Scope

This policy covers claude-buddy itself — the Rust in `src-tauri/`, the frontend
in `src/`, the build and release scripts, and the CI configuration. It does not
cover Claude Code, the Anthropic API, or macOS, all of which have their own
reporting channels and none of which this project can fix.
