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

## Privacy

From M3, Windowsill listens to the audio your PC plays, through WASAPI loopback. The analysis stays in memory: nothing is recorded, saved or sent anywhere.
