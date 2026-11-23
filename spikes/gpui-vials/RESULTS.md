# GPUI vs Tauri + Svelte: three vials on the desktop

Same scenario for both: 3 vials of the same size and place, blurred (acrylic) window, rounded corners,
20 s idle, then 10 s of scripted dragging (all three windows moved at ~60 Hz). Windows 11 26300, 5120x1440,
release builds. Two runs each (`results/*.json`); the table shows the average. Reproduce with `measure.ps1`.

| | Tauri + Svelte (now) | GPUI spike | Winner |
| --- | --- | --- | --- |
| Processes | 10 (1 app + 9 WebView2) | 1 | GPUI |
| RAM, private | ~170 MB | ~20 MB | GPUI, 8x less |
| RAM, working set (incl. shared) | ~620 MB | ~56 MB | GPUI, 11x less |
| GPU memory | ~245 MB | ~55 MB | GPUI, 4.5x less |
| Threads / handles | ~290 / ~4800 | 66 / 640 | GPUI |
| First window visible | 0.8 s | 0.5 s | GPUI (small) |
| CPU idle (% of one core) | ~1.1 % | ~1.3 % | tie (noise) |
| CPU while dragging | ~2.5 % | ~5 % | Tauri (small) |
| Exe size | 15 MB + shared WebView2 | 6 MB | GPUI |
| Blur + rounded corners | yes (Windows acrylic) | yes (same acrylic) | tie |
| Liquid glass (refraction) | possible (WebGL, done) | possible only with custom wgpu/shader work | Tauri today |
| Effort to adopt | exists | rewrite all UI, no widgets | Tauri |

Fair-comparison notes:
- Of the Tauri total, `vials.exe` itself (engine, tray, hooks) is 11.7 MB; the 9 WebView2 processes are 157 MB.
  A GPUI version would keep an engine, so the realistic saving is ~170 MB -> ~30 MB, about 5-6x, not 8x.
- The spike is UI only. Tauri's vials were empty (its items resolve against the real desktop); GPUI drew 12 tiles each.
- Idle CPU is dominated by noise: the first run of each app was higher than the second.
- In the Tauri screenshot two of three vials lost the blur (grey) while GPUI kept all three blurred; seen once, not investigated.
- Not tested: items with real icons, drag-and-drop out to Explorer, never-take-focus / always-on-bottom windows in GPUI.

## Phase 1 of the real port (`apps/vials-gpui`, reads the real config, real icons)

One run, same harness: first window 0.5 s, 1 process, ~21 MB private RAM, ~27 MB GPU memory, 68 threads,
idle CPU 0.9-1.7 % of one core. Screenshot: `results/vials-gpui.png`. Still read-only: no editing,
no hiding of desktop items, no tray (see `docs/gpui-migration.md`).
