# ScreenStitch

Monitors with different sizes or resolutions don't line up in Windows: move
the cursor from a 1440p screen to a 4K one and it jumps up or down, because
Windows connects the screens pixel by pixel. ScreenStitch connects them by
their real, physical size, so the cursor crosses exactly where your hand
expects it.

- **Works on its own.** Reads each monitor's real size from the monitor
  itself and guesses how they sit on your desk from the Windows arrangement.
- **Fix it in seconds.** Drag the screens so they match your desk. *Check
  alignment* draws one straight line across all your real screens: if it looks
  broken where two meet, drag until it's straight.
- **Never in your way.** It only acts at the moment the cursor leaves a screen.
  It never changes mouse speed, DPI or acceleration, and while a fullscreen game
  is in front it removes itself from the input path completely.
- **Light.** The part that runs in the tray is under 500 KB and uses about
  2 MB of memory. The settings window closes completely when you close it.

Hold **Ctrl** to cross the plain Windows way, or press **Ctrl+Alt+Shift+S** to
turn ScreenStitch on or off at any time.

## Install

Download the installer from the
[latest release](https://github.com/BraxtonElmer/ScreenStitch/releases/latest)
and run it. No admin rights are needed. There's also a portable zip if you'd
rather not install anything. ScreenStitch keeps itself up to date and asks
before installing a new version.

## Build

Needs Rust and Node.js.

```powershell
./scripts/build.ps1              # ready-to-run copy in dist/
./scripts/build.ps1 -Installer   # also builds the Windows installer
```

Then run `dist/ScreenStitch.exe`, or the installer from `target/release/bundle/nsis/`.

| Folder | What's in it |
| --- | --- |
| `crates/core` | Geometry, desk layout and the crossing engine. No Windows code, fully unit tested. |
| `crates/platform` | Monitor detection, EDID, settings file, start with Windows. |
| `crates/tray` | `ScreenStitch.exe`: tray icon and the mouse hook. |
| `settings` | The settings window (Tauri + Svelte). |

## License

ScreenStitch is free software, licensed under the [GNU General Public License v3.0](LICENSE).
