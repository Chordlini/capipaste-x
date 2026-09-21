# Capipaste for Windows and Linux

A port of [Capipaste](https://github.com/Chordlini/capipaste) — the Mac menu-bar app that turns a screenshot and what you say into a clipboard paste — built with Rust and Tauri v2.

This is an early build. It does the core loop:

**Ctrl+Shift+S** → drag over a region → mark it up (pen, arrow, box, blur, erase, undo) → **Copy**. The image goes on the clipboard, and a copy is saved in your Pictures/Capipaste folder.

Voice notes, screen reading and note tidying are Mac-only for now.

## Install

Download from [capipaste.com](https://capipaste.com): the Windows installer, or the `.AppImage` / `.deb` for Linux. Neither is code-signed yet, so Windows shows a SmartScreen warning ("More info" → "Run anyway").

## Build it yourself

Needs [Rust](https://rustup.rs). On Linux also `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`, `libxdo-dev`, `libxcb1-dev`, `libxrandr-dev` and `libdbus-1-dev`.

```sh
cd src-tauri
cargo run                          # run it
npx @tauri-apps/cli@2 build        # installers, in target/release/bundle
```

`ui/` is plain HTML with no build step: `select.html` is the region overlay, `card.html` the markup card. `src-tauri/src/lib.rs` is the whole backend — tray, hotkey, screen grab, clipboard.

## Linux notes

Everything works under X11. On Wayland the global hotkey and direct screen grabs are restricted, so a capture has to go through the desktop's screenshot portal — not wired up yet.

MIT licensed, like the Mac app.
