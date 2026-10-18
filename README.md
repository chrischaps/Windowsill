# Windowsill

A thin, living strip that rests on top of the Windows taskbar. It looks like a windowsill, with a potted plant, a glass jar and a candle on it, all in the light of the real sky. The light follows your local sun and weather. When music plays, the sill answers quietly: dust drifts in the beam, and the plant leans with the bass.

It asks for nothing. There are no notifications, no window to manage, and every click passes through. You glance at it the way you glance at a real window, and it tells you the time of day without a number.

**Status:** pre-v1. The work is tracked in [milestones](https://github.com/chrischaps/Windowsill/milestones) M1–M4. The full design is in [docs/proposal.md](docs/proposal.md).

## Build

```powershell
cargo run --release
cargo test
```

Windows 10/11 only for now. It is built on Rust, winit, wgpu and (from M3) cpal, the same stack as [Seoul](https://github.com/chrischaps/Seoul).

## Settings

Copy [windowsill.example.toml](windowsill.example.toml) to `windowsill.toml` next to the exe, or to `%APPDATA%\Windowsill`, to set its frame rate and the ledge's colours. The sill learns where it is from Windows' location service; set `latitude` and `longitude` there to choose the place yourself. The sill re-reads it on every save. While the source is on disk, it also reloads `src/glass.wgsl` on save, so the ledge can be tuned while it runs.

## Checking a whole day

```powershell
cargo run --release -- --day-in 60s            # a whole day in a minute, from now
cargo run --release -- --day-in 2m --at 04:00  # from before dawn
cargo run --release -- --at 19:30              # held at dusk
cargo run --release -- --lat <deg> --lon <deg> # somewhere else's sky
```

Only the sky's clock is changed. The ease speeds up with it, so a sped-up day looks like a real one, only shorter. The candle's flicker and the dust keep real time, as the music does. While sped up, the log notes the sun, moon and candle on every hour.

## Privacy

From M3, Windowsill listens to the audio your PC plays, through WASAPI loopback. The analysis stays in memory: nothing is recorded, saved or sent anywhere.
