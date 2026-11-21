# Changelog

All notable changes to Tessera are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [0.2.0] - 2026-11-21

### Added

- Options panel, opened with a right click on the tray icon, to choose the shortcut that opens the grid. Every key is marked as free, taken, or documented as a Windows shortcut for the modifiers you tick, and the panel will not save a combination that is taken.
- The chosen shortcut is remembered between runs. If it cannot be registered at startup, Tessera falls back to Win+Shift+G, then to Win+Ctrl+Alt+G, and shows a notification.
- *Options…* entry in the left-click tray menu.
- Windows installer built with Inno Setup: per-user by default, Start menu entry, upgrades in place, clean uninstall.
- MIT license, a complete README, and GitHub Actions workflows for checks and releases.

### Changed

- A right click on the tray icon now opens the options panel. A left click still opens the menu.

## [0.1.0] - 2026-11-16

### Added

- First version: a 6×6 grid opened with Win+Shift+G, mouse and keyboard selection, snapping of the focused window to the monitor's work area, tray icon with *Start with Windows* and *Exit*, and a log file.
