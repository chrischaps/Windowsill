# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`windowsill` is a single Rust binary: a 90 px clear strip just above the bottom taskbar on the primary monitor, with a plant, a jar and a candle standing on the taskbar's top edge in the light of the real sun, moon and weather, responding gently to system audio. The taskbar is the sill; there is no drawn ledge. It uses Seoul's stack: winit 0.30, wgpu 30, and cpal 0.18 from M3. Windows only for v1. The full design is in `docs/proposal.md`.

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
2. **Never asks.** No notifications, sounds, badges or prompts. Clicks pass through everywhere but the things themselves. Two interactions, both the user's to start: pick a thing up and carry it along the taskbar (`carry.rs`), and hover-and-hold over the candle for a tiny tray menu. A hand moving a thing is the one exception to rule 1's pace; what it disturbs (neighbours making room, the slosh, the flame) still eases.
3. **True to the world.** Real sun angle, real weather, real moon phase. If something is unknown, show plain daylight. Never invent it.
4. **Steps aside.** Hide during fullscreen apps and presentations, when the taskbar auto-hides, and in any unsupported taskbar layout.
5. **Light on the machine.** Under 1% CPU. 15 fps idle, 30 fps with music. Stop completely on screen lock.
6. **Drawn in code.** No art assets. Everything is shapes and WGSL shaders, so it stays crisp at any DPI.

## Architecture

Four inputs (clock/sun, audio, weather, window/taskbar state) feed one small sill state. The state is eased every frame and drawn by a renderer into a layered, click-through window (`WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`). It is placed with `SHAppBarMessage(ABM_GETTASKBARPOS)` and re-placed on `WM_DISPLAYCHANGE` and `WM_DPICHANGED`. While the polled cursor is over a thing, `WS_EX_TRANSPARENT` comes off (through winit's `set_cursor_hittest`, then `win32::make_glass(window, touchable)`) so the press lands on the sill; it goes back on the moment the cursor leaves. Where things were set down is kept in `%APPDATA%\Windowsill\arranged.toml`.

Reused from elsewhere:
- **Seoul** (`C:\Users\chris\dev\Seoul`): copy `src/audio/capture.rs` as is. Copy `analysis.rs` and `features.rs`, then trim them to `bass_att`, `beat`, `beat_phase` and `kick`. Seoul's CLAUDE.md has the WASAPI loopback gotcha: build the input stream on the *default output device* with `default_output_config()`. Use the same `notify` hot-reload pattern for TOML and WGSL.
- **Fishbowl**: the palette keyframes for the day/night cycle (port the idea, not the Lua).
- **Flint stays out on purpose.** A flat strip doesn't need a 3D engine.

## Workflow

Work is tracked in GitHub issues and milestones on `chrischaps/Windowsill`. Pick the lowest-numbered open issue in the earliest open milestone, branch `issue-<n>`, and open a PR with `Closes #<n>`. Issues labelled `question` must be settled with Chris before the issues they block.
