# Changelog

All notable changes to ScreenStitch are listed here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.2] - 2026-10-03

- The settings window now shows a monitor plugged in or out while it's open,
  instead of only after it's reopened.

## [0.2.1] - 2026-10-02

- Plugging in another monitor no longer rearranges the screens you already
  lined up. The new screen is placed next to them, and only the new one is
  guessed from the Windows arrangement.

## [0.2.0] - 2026-10-02

- New *Stop at gaps* option: where part of a screen's edge has no other screen
  beside it, the cursor stops there instead of hopping to the nearest point of
  the next screen.
- Seams between screens are drawn as a thread weaving through needle holes,
  sewn in when a screen is placed.
- The window background is now frosted glass or solid; Mica was removed.
- Footer credits and a Ko-fi link.

## [0.1.0] - 2026-09-29

First release.

- The cursor crosses between monitors of different sizes, resolutions and
  scaling at the physically correct height, based on each monitor's real size.
- Monitor sizes are read from the monitors themselves, and a desk layout is
  guessed from the Windows arrangement, so it works without setup.
- Settings window to drag screens into place, with changes applied live.
- *Check alignment* draws a line across all real screens to verify the layout.
- Correct a monitor's size, line up rows, undo, and reset to automatic.
- Hold Ctrl to cross the plain Windows way; Ctrl+Alt+Shift+S turns
  ScreenStitch on or off.
- Steps aside completely while a fullscreen game is in front, and never changes
  mouse speed, DPI or acceleration.
- Separate layouts for each set of connected monitors.
- Light and dark themes, accent colours, and frosted glass, Mica or solid
  window backgrounds.
- Start with Windows, a per-user installer, and automatic updates.
