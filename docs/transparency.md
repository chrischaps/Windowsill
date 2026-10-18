# Per-pixel transparency on Windows (#4)

The sill fades to fully see-through above the ledge, so the swapchain's alpha has to reach the desktop. This was the proposal's top risk. This note records what each path did and which one the sill uses.

**Machine:** Windows 11 (build 26300), NVIDIA GeForce RTX 4070, driver 610.88, 3840×2160 at 150%. **Stack:** wgpu 30.0.1, winit 0.30.

## What each path did

`WINDOWSILL_GLASS=visual|vulkan|hwnd` picks a path, so the comparison can be run again on other hardware. The test pane is a wood-coloured band that is opaque at the bottom and fades to nothing at the top (`src/glass.wgsl`).

| Path | Alpha modes the surface reports | Result | CPU (one core, 15 fps) | GPU 3D | Working set |
| --- | --- | --- | --- | --- | --- |
| **DX12 → DirectComposition visual** (`Dx12SwapchainKind::DxgiFromVisual`) | Auto, Inherit, Opaque, PostMultiplied, PreMultiplied | ✅ The gradient shows the desktop through it | ~0.9% | ~0.07% | ~153 MB |
| Vulkan | Opaque, PreMultiplied | ✅ Also works on this driver | ~1.7% | ~0.12% | ~170 MB |
| DX12 → HWND (`DxgiFromHwnd`, wgpu's default) | Opaque only | ❌ The faded part is solid black | ~1.9% | ~0.06% | ~153 MB |

Each figure is one 10-second sample of process CPU time and the `GPU Engine … engtype_3D` counter, with the window spanning the monitor at 3840×180. "~0.9% of one core" is about 0.04% of this machine's 24 threads, well under the 1% budget. #24 will make this a proper check.

Clicks fall through on all three paths: `WindowFromPoint` inside the strip returns the window underneath.

## The choice: DX12 into a DirectComposition visual

- wgpu 30 supports it directly. Set `backend_options.dx12.presentation_system = DxgiFromVisual`, and wgpu builds the DComp device, target and visual over the HWND, then presents into it with a flip-model composition swapchain. We need no Win32 composition code of our own.
- It was the cheapest path on CPU.
- It does not depend on the Vulkan driver. On Windows, a Vulkan driver is free to offer only `Opaque`, and other vendors' drivers may do just that, so premultiplied alpha there is luck, not a contract. A DComp swapchain with `DXGI_ALPHA_MODE_PREMULTIPLIED` is the contract.
- The `UpdateLayeredWindow` fallback (read back, CPU copy) was not needed, so it was not built.

The cost: RenderDoc can't capture DComp swapchains. To debug a frame, run with `WINDOWSILL_GLASS=hwnd` (opaque, but capturable).

## What makes it work

1. **No redirection bitmap.** The window is made with `with_no_redirection_bitmap(true)` (`WS_EX_NOREDIRECTIONBITMAP`), so no GDI surface sits under the visual. Only the visual path asks for this.
2. **Premultiplied colour, non-sRGB format.** The surface uses `CompositeAlphaMode::PreMultiplied` and a plain `Bgra8Unorm`-style format. The shader writes `rgb * a, a`, and DWM blends exactly that. The clear colour is `TRANSPARENT`.
3. **Layered + `LWA_ALPHA` 255 is harmless.** `make_glass` still calls `SetLayeredWindowAttributes(255, LWA_ALPHA)`, which click-through needs. A whole-window alpha of 255 leaves the visual's per-pixel alpha alone.

## For later

- `PowerPreference::LowPower` still chose the 4070 here, the only GPU. On a hybrid laptop it should choose the integrated GPU.
- The ~150 MB working set is mostly the D3D12 driver. Watch it in #24 before deciding whether it matters.
- If a machine fails at the visual path, `WINDOWSILL_GLASS=vulkan` is the first thing to try. Then build the `UpdateLayeredWindow` fallback.
