# jp2-winthumb

A Windows WIC decoder that makes File Explorer render thumbnails for JPEG 2000 (`.jp2`) files. Also makes JP2 images viewable in any WIC-capable Windows app (Photos, Paint, Print Preview, etc.).

## What this is

Windows has no native JPEG 2000 support — `.jp2` files show up as generic icons in Explorer with no preview, no thumbnail in the file list, nothing in the preview pane. This project ships a single Rust DLL that plugs into Windows Imaging Component (WIC) and fills that gap.

Because it's a WIC decoder (not a direct thumbnail provider), thumbnails work *everywhere* Windows uses WIC: Explorer's folder view and preview pane, the system file picker, and any third-party app built against WIC.

<img width="290" height="491" alt="image" src="https://github.com/user-attachments/assets/5dfe21f4-eca3-40b0-9890-9ffad4a0fa29" />


## Credits

This is a fork of **[jxl-winthumb](https://github.com/saschanaz/jxl-winthumb)** by Kagami Sascha Rosylight, adapted to decode JPEG 2000 instead of JPEG XL. The COM plumbing, registry handling, and build system are essentially theirs — the decoder swap and pyramid optimization are the only substantive changes. Huge thanks to the jxl-winthumb authors; this would have been a weekend-long rabbit hole otherwise.

Original jxl-winthumb is ISC-licensed, which is retained (see [LICENSE](LICENSE)).

## Features

- Single self-contained ~650 KB DLL, no external runtime dependencies
- Uses a pure-Rust OpenJPEG port ([`openjp2`](https://crates.io/crates/openjp2)) — no C toolchain needed at runtime, no `openjp2.dll` in `System32`
- Exploits JP2's resolution pyramid via a two-pass decode: peeks the header to get original dimensions, then decodes at a matched reduce level (`rlevel`). Thumbnails for 10k × 10k aerial tiles render in well under a second.
- Registers the standard JP2 container signature (`00 00 00 0C 6A 50 20 20 0D 0A 87 0A`) so Explorer can invoke the decoder even without file extensions

## Architecture

```
Explorer thumbnail request
       ↓
CLSID_PhotoThumbnailProvider  (built-in Windows)
       ↓
WIC asks "which decoder handles .jp2?"
       ↓
Our registered IWICBitmapDecoder (this DLL)
       ↓
  Pass 1: peek header via openjp2 with reduce=8
         → get orig_width, orig_height
  Pass 2: decode via openjp2 with reduce=N
         where N is chosen so max(w,h) / 2^N ≤ 512
       ↓
Return RGBA8 frame to WIC → Explorer renders thumbnail
```

The `reduce=N` trick is the key optimization. Decoding a 10000×10000 aerial JP2 at full resolution to answer Explorer's 256px thumbnail request takes ~3 seconds; decoding at reduce=5 (giving ~312×312 pixels) takes under 200 ms.

## Build

### Prerequisites

- **Rust** (stable, recent). Install from <https://rustup.rs>.
- **Visual Studio Build Tools** with the "Desktop development with C++" workload (for the MSVC linker that Rust targets on Windows).

Verify in a fresh PowerShell:
```powershell
rustc --version
where.exe link   # should resolve to an MSVC link.exe
```

### Compile

```powershell
git clone https://github.com/MutantKiwi/jp2-winthumb
cd jp2-winthumb
cargo build --release
```

First build takes ~30 seconds on typical hardware (openjp2 is pure Rust so there's no C compilation). Artifact: `target\release\jp2_winthumb.dll` (~650 KB).

## Install

**Do this from PowerShell running as Administrator.**

```powershell
cd path\to\jp2-winthumb\target\release
regsvr32 jp2_winthumb.dll
```

Expected popup: "DllRegisterServer in jp2_winthumb.dll succeeded."

**Important:** Windows caches the registration against the absolute path of the DLL. Don't move the DLL after registering. If you need to relocate it:

```powershell
# From the original location:
regsvr32 /u jp2_winthumb.dll

# Move the DLL to the new location, then from there:
regsvr32 jp2_winthumb.dll
```

### Kick Explorer so it picks up the new registration

```powershell
taskkill /f /im explorer.exe
start explorer.exe
```

### Clear the stale thumbnail cache

Only matters if you've been browsing `.jp2` folders before installing this. Windows caches a "no thumbnail" result per file and won't retry just because a handler appeared.

```powershell
cleanmgr /d C:
```

Tick **only** "Thumbnails" → OK.

## Uninstall

```powershell
cd path\to\jp2-winthumb\target\release
regsvr32 /u jp2_winthumb.dll
taskkill /f /im explorer.exe
start explorer.exe
```

## Troubleshooting

### `regsvr32` returns 0x80070005

Access denied. Your PowerShell isn't elevated — relaunch as Administrator.

### `regsvr32` returns 0x80040154

Architecture mismatch. The DLL is x64; you probably ran the 32-bit `regsvr32`. On 64-bit Windows, the default `regsvr32` in `C:\Windows\System32\` is actually the 64-bit one, so this only bites if you're on 32-bit Windows (very rare these days).

### Registration succeeds but thumbnails don't appear

Clear the thumbnail cache (see above). Windows's per-file "no preview available" caching is sticky.

### Double-clicking `.jp2` opened my viewer, now it doesn't

The registration process creates a ProgID that Windows may promote to the default for `.jp2`, hijacking your viewer association. To restore your viewer as the default **without** losing thumbnails:

1. Register your viewer's ProgID (see the viewer's README for the full snippet)
2. Use [SetUserFTA](https://setuserfta.com/) (free personal edition) to set it as the default:

```powershell
.\SetUserFTA.exe .jp2 YourViewer.File
```

This works around Windows 11's UCPD.sys protection that prevents direct registry edits to the UserChoice key.

### Build fails with linker errors

You're missing Visual Studio Build Tools with the C++ workload. Install from <https://visualstudio.microsoft.com/visual-cpp-build-tools/>, tick "Desktop development with C++".

### Some `.jp2` files thumbnail, others don't

This decoder only registers the `.jp2` container format (files starting with the signature box `00 00 00 0C 6A 50 20 20 0D 0A 87 0A`). Raw `.j2k` codestreams and `.jpx`/`.jpf` variants aren't currently supported. If you need them, file an issue or tweak `src/registry.rs` to register additional extensions and bytestream patterns.

### Debug trace logs

When built with `cargo build` (without `--release`), the DLL writes trace logs to `debug-<timestamp>.log` in the project root each time Explorer invokes it. Useful for diagnosing load failures.

## Companion: standalone viewer

Pairs with **[jp2-viewer](https://github.com/MutantKiwi/jp2-viewer)** — a Python viewer for when you want more than a thumbnail. Zoom, pan, rotate, geo coord readout.

## License

ISC (inherited from upstream jxl-winthumb). See [LICENSE](LICENSE).

## Built with

- [windows-rs](https://github.com/microsoft/windows-rs) — Rust bindings to the Win32 API
- [jpeg2k](https://crates.io/crates/jpeg2k) — safe wrapper over OpenJPEG
- [openjp2](https://crates.io/crates/openjp2) — pure-Rust port of OpenJPEG
- [image](https://crates.io/crates/image) — for RGBA conversion
- [winreg](https://crates.io/crates/winreg) — registry access for DllRegisterServer
