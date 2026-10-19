# Windowsill — Proposal

Oct 17, 2026 · @Chris

## Summary

Windowsill is a thin, living strip that rests on top of the Windows taskbar. It looks like a windowsill, with a few small things on it in the light of the real sky. The light follows your local sun and weather. When music plays, the sill answers quietly: dust drifts in the beam, a plant leans with the bass.

It asks for nothing. It sends no notifications, has no window to manage, and lets every click pass through except on the things themselves, which you can move along the sill. You look at it the way you glance at a real window, and it tells you the time of day without a number.

It is worth building because most of it already exists. Seoul has the system-audio capture and beat analysis. Fishbowl worked out the day-and-night rhythm. A v1 is roughly two to three weekends of new code, which makes it one of the more finishable ideas on the list.

## The experience

The taskbar is the sill. Three objects stand on its top edge: a potted plant, a small glass jar, and a candle. Everything around them is clear, so your desktop shows through as if it were the glass. (The first version drew a wooden ledge 120 px tall above the taskbar; it took more of the screen than it gave back, and the taskbar already is a ledge.)

You can pick any of them up and carry it along the taskbar. It lifts a few pixels and tips with the way you carry it; the flame and the leaves trail back and the water sloshes. Set it down on another and that one slides aside to make room. They stay where you leave them.

A day on the sill:

- **Morning.** Low, gold light comes in at a steep angle and casts long shadows to one side. Real sunrise sets the timing, from your latitude and longitude.
- **Midday.** The light is short and white, and catches both edges of every object from behind.
- **Rain.** If the weather says rain, drops bead and run down the glass edge and the light goes grey. Snow settles in a thin line along the taskbar's edge.
- **Music.** When system audio plays, dust motes appear in the beam and drift on the beat. The plant's leaves lean slightly on bass hits. The jar rings faintly on accents. Nothing flashes; it all happens at the speed of breath.
- **Evening.** The light warms and fades. The candle lights itself at local sunset.
- **Night.** Moonlight in the right phase, cool and dim. The candle burns down over the evening and is fresh again the next morning.

Over weeks the plant grows one leaf for each day you use the computer. After about a month it flowers. Leave for a holiday and it wilts a little, then recovers when you return. Apart from where you last set things down, it is the only memory Windowsill keeps.

## Design principles

1. **Peripheral, never central.** It must be readable in a glance from the corner of your eye. Any change takes at least two seconds; nothing moves faster than a leaf in a draft.
2. **Never asks.** No notifications, sounds, badges or prompts. Clicks pass through everywhere but the things themselves. The only interactions are yours to begin: carrying a thing along the sill, and a hover-and-hold on the candle that shows a tiny tray menu.
3. **True to the world.** The light is the real sun's angle, the weather is the real weather, and the moon is in the real phase. If it can't know something, it shows plain daylight rather than inventing it.
4. **Steps aside.** It hides itself during fullscreen apps, games and presentations, and when the taskbar auto-hides.
5. **Light on the machine.** It aims for under 1% CPU and about 15 fps when there's no music, and 30 fps with music. It sleeps completely when the screen is locked.
6. **Drawn in code.** Like Bridge's cards, everything is made from shapes and shaders, with no art assets. The look stays crisp at any DPI and is easy to retheme.

## Architecture

Windowsill is a single Rust binary built on the same stack as Seoul: winit, wgpu 30 and cpal. Four inputs feed one small state, and the state is eased every frame, so nothing ever snaps. A renderer draws that state into a layered window on the taskbar edge.

&#91;embedded content: Windowsill architecture · 4 inputs, 1 state, 1 window\]

- **Window.** winit with no decorations, plus Win32 extended styles: `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` for click-through, no taskbar button and never taking focus. `SHAppBarMessage(ABM_GETTASKBARPOS)` places it, re-checked on `WM_DISPLAYCHANGE` and `WM_DPICHANGED`.
- **Light.** The NOAA solar position formula takes latitude, longitude and the time, and gives elevation and azimuth. Elevation sets beam angle and colour temperature; azimuth sets which side shadows fall on.
- **Audio.** Seoul's `LoopbackCapture` and `Analyzer` unchanged. The sill reads `bass_att`, `beat`, `beat_phase` and `kick`, and nothing else.
- **Stepping aside.** `SHQueryUserNotificationState` reports fullscreen apps and presentations; `WTSRegisterSessionNotification` reports screen lock.

## What it reuses

About half of v1 can be lifted from projects you already have.

| From | What carries over | How |
| --- | --- | --- |
| Seoul | `audio/capture.rs`: WASAPI loopback that reconnects itself | Copy as is |
| Seoul | `audio/analysis.rs` + `features.rs`: bass/mid/treble, kick/snare/hat, BPM, beat phase | Copy, then trim to the fields the sill uses |
| Seoul | Hot-reloading TOML and WGSL through `notify` | The same pattern, for tuning light and palette live |
| Seoul / Soba | `audio/link.rs`: Soba's OSC link | Optional: when Soba is open, its clock drives the motes |
| Fishbowl | The 10-minute day/night cycle: palette keyframes, named living things | Port the idea and the palette, not the Lua |
| ShaderPlayground | A place to sketch the light beam and caustics before writing WGSL | Prototype there, then port |
| Bridge | Lamp-lit warmth and everything drawn in code | Visual reference |

Flint stays out on purpose. A flat strip of light doesn't need a 3D engine, and Seoul's plain wgpu and winit stack is smaller.

## Milestones

v1 ends at M4. After that, everything is optional and can ship one at a time.

1. **M1 · The glass.** A transparent, click-through, always-on-top strip that sits above the taskbar and follows it if it moves. Done when it draws a flat colour band and survives a DPI change and a monitor unplug.
2. **M2 · The light.** The sun's position from latitude, longitude and the clock drives the beam's angle, colour and shadows, and the candle lights at sunset. Done when a sped-up day (`--day-in 60s`) looks right from dawn to night.
3. **M3 · The music.** Seoul's capture and analysis feed the dust motes, the plant's lean and the jar's ring. Done when a song plays in the browser and the sill responds within 50 ms with nothing that flickers.
4. **M4 · Stepping aside.** It hides during fullscreen, auto-hide and screen lock, adds a tray menu, and starts at login. Done when a week of daily use brings no reason to close it. **Ship v1 here.**
5. **M5 · Weather.** Open-Meteo every 30 minutes (free, no key): rain on the glass, snow along the taskbar's edge, grey light.
6. **M6 · The plant remembers.** One leaf per day of use, a flower after about 30, and a little wilt after time away. Saved as a small TOML file.
7. **Later.** Swappable sill objects as presets in Seoul's style (TOML + WGSL), Soba link sync, macOS above the Dock, and a short loop filmed for chaps.dev.

## Risks and open questions

| Risk | Why it matters | Fallback |
| --- | --- | --- |
| Per-pixel transparency with wgpu on Windows | Depends on the backend and the compositor; DX12 may give an opaque swapchain | Render offscreen and present via `UpdateLayeredWindow` (CPU copy, fine at 15 fps), or try Vulkan's premultiplied alpha mode |
| Click-through vs. the hover menu | `WS_EX_TRANSPARENT` drops all mouse input, including hover | Poll the cursor position from a timer, and turn click-through off only while the cursor is over one of the things, so it can be picked up; back on the moment it leaves |
| Taskbar edge cases | Taskbar on the side or top, auto-hide, several monitors, Windows 11 centred icons | v1 supports only a bottom taskbar on the primary monitor and hides itself in every other case |
| Battery and GPU cost | A desktop toy that drains a laptop gets uninstalled | Frame cap, a full stop on lock, and a perf check in CI the way Soba has one |
| Loopback privacy | It listens to everything the PC plays | Analysis stays in memory only; nothing is recorded or sent. Say so on the About page |

Open questions:

- [x] One sill with fixed objects, or a small choice (plant, cat, bonsai)? A fixed set: plant, jar and candle (#33). A choice can come with the presets.
- [ ] Should anything ever invite a pause, such as the light turning briefly on the hour? It risks breaking "never asks".
- [ ] Windows only for v1, or keep macOS in mind from the start? Seoul's capture already supports CoreAudio.
- [ ] Location: ask for a city once, or work out a rough location from the Windows time zone?
