# Windowsill

A potted plant, a glass jar and a candle standing on the Windows taskbar, as if it were a windowsill, all in the light of the real sky. The light follows your local sun and weather. When music plays, they answer quietly: dust drifts in the light, and the plant leans with the bass.

It asks for nothing. There are no notifications, no window to manage, and every click passes through everywhere but the things themselves. Those you can pick up and set down somewhere else along the taskbar, and they stay where you put them. You glance at it the way you glance at a real window, and it tells you the time of day without a number.

**Status:** pre-v1. The work is tracked in [milestones](https://github.com/chrischaps/Windowsill/milestones) M1–M4. The full design is in [docs/proposal.md](docs/proposal.md).

## Build

```powershell
cargo run --release
cargo test
```

Windows 10/11 only for now. It is built on Rust, winit, wgpu and (from M3) cpal, the same stack as [Seoul](https://github.com/chrischaps/Seoul).

## Settings

Copy [windowsill.example.toml](windowsill.example.toml) to `windowsill.toml` next to the exe, or to `%APPDATA%\Windowsill`, to set its frame rate, where things stand to begin with, and their colours. The sill learns where it is from Windows' location service; set `latitude` and `longitude` there to choose the place yourself. The sill re-reads it on every save. While the source is on disk, it also reloads `src/glass.wgsl` on save, so the things can be retuned while it runs.

## Checking a whole day

```powershell
cargo run --release -- --day-in 60s            # a whole day in a minute, from now
cargo run --release -- --day-in 2m --at 04:00  # from before dawn
cargo run --release -- --at 19:30              # held at dusk
cargo run --release -- --lat <deg> --lon <deg> # somewhere else's sky
```

Only the sky's clock is changed. The ease speeds up with it, so a sped-up day looks like a real one, only shorter. The candle's flicker and the dust keep real time, as the music does. While sped up, the log notes the sun, moon and candle on every hour.

## Light on the machine

The target is under 1% CPU, drawing at 15 fps when idle and 30 fps while music plays. Measured over a minute of the running sill (Task Manager's share is the whole machine's, here 24 threads on an RTX 4070 desktop):

| | of one core | Task Manager |
|---|---|---|
| Idle, 15 fps | 2.4% | 0.10% |
| Music, 30 fps | 3.0% | 0.13% |

While something fullscreen is up it draws nothing, and while the screen is locked it does nothing at all.

`--bench` draws frames offscreen, idle and with a synthesized beat, doing all of a real frame's work, and reports the CPU time of each:

```powershell
cargo run --release -- --bench 3000              # about 0.2 ms a frame on the desktop above
cargo run --release -- --bench 300 --budget 650  # what CI runs
```

A frame is only part of what the running sill costs; the rest is listening, presenting and looking at the desktop, which is why the table above is higher. CI has no GPU, so it draws on Windows' software renderer, where the shader runs on the CPU too. It fails if a frame takes more than about twice what the runner usually takes, so a shader or a loop that has grown a lot is caught.

## Privacy

From M3, Windowsill listens to the audio your PC plays, through WASAPI loopback. The analysis stays in memory: nothing is recorded, saved or sent anywhere.
