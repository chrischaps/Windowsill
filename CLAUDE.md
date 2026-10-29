# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`windowsill` is a single Rust binary: a 120 px strip that sits just above the bottom taskbar on the primary monitor and shows a wooden ledge (plant, jar, candle) in the light of the real sun, moon and weather, responding gently to system audio. It uses Seoul's stack: winit 0.30, wgpu 30, and cpal 0.18 from M3. Windows only for v1. The full design is in `docs/proposal.md`.

## Commands

```bash
cargo run --release
cargo run --release -- --day-in 60s   # a whole day in a minute; --at 06:30 starts or holds a time
cargo test
cargo clippy -- -D warnings   # CI runs this
RUST_LOG=windowsill=debug cargo run --release
```

## Design rules (from the proposal; treat as hard constraints)

1. **Peripheral, never central.** Any visible change takes ≥2 s. Nothing moves faster than a leaf in a draft. All state is eased every frame; nothing snaps.
2. **Never asks.** No notifications, sounds, badges or prompts. Clicks pass through. The only interaction is hover-and-hold over the candle for a tiny tray menu.
3. **True to the world.** Real sun angle, real weather, real moon phase. If something is unknown, show plain daylight. Never invent it.
4. **Steps aside.** Hide during fullscreen apps and presentations, when the taskbar auto-hides, and in any unsupported taskbar layout.
5. **Light on the machine.** Under 1% CPU. 15 fps idle, 30 fps with music. Stop completely on screen lock.
6. **Drawn in code.** No art assets. Everything is shapes and WGSL shaders, so it stays crisp at any DPI.

## Architecture

Four inputs (clock/sun, audio, weather, window/taskbar state) feed one small sill state. The state is eased every frame and drawn by a renderer into a layered, click-through window (`WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`). It is placed with `SHAppBarMessage(ABM_GETTASKBARPOS)` and re-placed on `WM_DISPLAYCHANGE` and `WM_DPICHANGED`.

Reused from elsewhere:
- **Seoul** (`C:\Users\chris\dev\Seoul`): copy `src/audio/capture.rs` as is. Copy `analysis.rs` and `features.rs`, then trim them to `bass_att`, `beat`, `beat_phase` and `kick`. Seoul's CLAUDE.md has the WASAPI loopback gotcha: build the input stream on the *default output device* with `default_output_config()`. Use the same `notify` hot-reload pattern for TOML and WGSL.
- **Fishbowl**: the palette keyframes for the day/night cycle (port the idea, not the Lua).
- **Flint stays out on purpose.** A flat strip doesn't need a 3D engine.

## Workflow

Work is tracked in GitHub issues and milestones on `chrischaps/Windowsill`. Pick the lowest-numbered open issue in the earliest open milestone, branch `issue-<n>`, and open a PR with `Closes #<n>`. Issues labelled `question` must be settled with Chris before the issues they block.

<!-- claims:begin (from chrischaps/Dashboard skills/setup-claims; re-run its install.sh to update) -->
## Claiming issues

Agents on several machines work this repo, all as the same GitHub user, so an issue is claimed before anyone works on it. A claim is the branch `claim/issue-<n>` on origin, holding one empty commit that names the agent and host. Git accepts only the first push of a new branch, so exactly one claimant wins. `scripts/issue.sh` does the whole cycle (bash; on Windows it runs under Git Bash):

```bash
scripts/issue.sh list                  # who holds what, on every machine, and for how long
scripts/issue.sh claim <n>             # claim and make the worktree ../Windowsill-<n>; exit 2: taken, pick another
scripts/issue.sh claim <n> --here      # claim for the worktree you're already in (a desktop-app worktree)
scripts/issue.sh renew <n>             # at the start of each session on long work
scripts/issue.sh land <n>              # verify, push the branch and open a PR to main; release <n> once it merges
scripts/issue.sh release <n>           # giving up: push your branch and comment where it stands first
```

- **A hook enforces it.** `.claude/settings.json` runs `scripts/issue-guard.sh` before every `git commit` and `git push`. It refuses issue work (a branch named `issue-<n>`, or a commit saying `Closes #<n>`) unless this session holds the claim, from the worktree it was claimed for. A chore with no issue (a typo, a dependency bump) needs no claim: keep it off `issue-<n>` branches and out of `Closes` lines. A SessionStart hook lists the current claims.
- **Only the session that claimed an issue renews, releases or lands it,** and only from its worktree. An existing worktree for an issue you didn't claim is another session's: leave it alone. Never delete another agent's `claim/*` branch.
- **Claims go stale after 24 hours.** `claim <n> --steal` takes over a stale one; check the issue's comments and `origin/issue-<n>` for work in progress first, and continue from it. Taking a younger claim is Chris's call only: `claim <n> --steal --override "<his reason>"`.
- Settings for this repo are in `scripts/issue.conf`. The tool is vendored from `skills/setup-claims` in chrischaps/Dashboard: change it there, not here. `bash scripts/issue-test.sh` tests it against a throwaway remote.
<!-- claims:end -->
