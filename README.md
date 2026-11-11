<p align="center">
  <img src="crates/app/icons/128x128@2x.png" width="128" height="128" alt="Lapsewright icon">
</p>

<h1 align="center">Lapsewright</h1>

<p align="center">
  Turn a folder of photos into a timelapse video. No ffmpeg commands to type.
</p>

<p align="center">
  <a href="https://github.com/diopterme/lapsewright/releases/latest"><img src="https://img.shields.io/github/v/release/diopterme/lapsewright" alt="Latest release"></a>
  <a href="https://github.com/diopterme/lapsewright/releases"><img src="https://img.shields.io/github/downloads/diopterme/lapsewright/total" alt="Downloads"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/diopterme/lapsewright" alt="License: GPL-3.0"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey" alt="Platform: macOS | Windows">
</p>

<p align="center">
  English | <a href="README.ko.md">한국어</a>
</p>

## Features

- **Photos:** JPG, PNG, JPEG XL and TIFF (16-bit too), one format per folder
- **Video:** H.264, HEVC or ProRes; SDR, HLG or HDR (PQ)
- **Settings:** size, frame rate, quality and motion blur; drag the frame to choose what to crop
- **Metadata:** keeps the photos' date, camera, lens, location and copyright in the video
- **Queue:** add several folders and get a notification when each one is done (macOS)
- **Languages:** Korean and English, following the system language

## Installation

Download the latest version from [Releases](https://github.com/diopterme/lapsewright/releases/latest). ffmpeg is built in.

| Platform | File | Notes |
| --- | --- | --- |
| macOS (Apple Silicon) | `Lapsewright_<version>_aarch64.dmg` | Open it and drag Lapsewright to Applications. Signed and notarized by Apple. |
| Windows 10/11 (64-bit) | `Lapsewright_<version>_x64-setup.exe` | Not signed. If "Windows protected your PC" appears, click **More info**, then **Run anyway**. |

## Building from source

### Requirements

- [Rust](https://rustup.rs) (edition 2024)
- [Node.js](https://nodejs.org) 22
- Tauri CLI 2: `cargo install tauri-cli --version "^2"`
- ffmpeg 9.0 or newer (on macOS: `brew install ffmpeg-full`)

### Run

```sh
(cd crates/app/ui && npm install)
(cd crates/app && LAPSEWRIGHT_FFMPEG=/opt/homebrew/bin/ffmpeg cargo tauri dev)
```

During development the app uses the ffmpeg given in `LAPSEWRIGHT_FFMPEG`.

### Test

```sh
cargo test --workspace
(cd crates/app/ui && npm run check && node --test src/lib/)
```

### Package

Both platforms build FFmpeg from source with `crates/app/package/ffmpeg.py`, turning on only the libraries the app uses: x264, x265, libjxl and zimg.

**macOS:** `brew install pkgconf x264 x265 jpeg-xl zimg`, then `python3 crates/app/package/mac.py` builds the `.dmg`. It puts FFmpeg and those Homebrew libraries into the app, writes their license notices into the app, then signs and notarizes it.
`python3 crates/app/package/licenses.py` collects the source code of those parts for the release page.

**Windows:** in an [MSYS2](https://www.msys2.org) UCRT64 shell, run `python crates/app/package/windows.py` (the MSYS2 packages it needs are listed at the top of the file). It copies FFmpeg, its DLLs and license notices into `crates/app/ffmpeg/` and collects their source code.
Then run `cargo tauri build` in `crates/app`.

## Command line

`crates/cli` is the same engine without the window:

```sh
cargo build -p lapsewright-cli
target/debug/lapsewright --ffmpeg /opt/homebrew/bin/ffmpeg convert <photo folder>
target/debug/lapsewright --help
```

## Privacy

Lapsewright works offline. The only network request is the place search, which sends the place name you type to [OpenStreetMap Nominatim](https://nominatim.openstreetmap.org).

## Support

Lapsewright is free. If it saves you time, you can [sponsor it on GitHub](https://github.com/sponsors/diopterme).

## License

Lapsewright is free software under the [GNU General Public License v3.0 or later](LICENSE).

The apps include [FFmpeg](https://ffmpeg.org) (GPLv3 build) and the libraries it uses, each under its own license.
Their notices are inside the app (`Lapsewright.app/Contents/Resources/licenses` on macOS, the `licenses` folder next to `Lapsewright.exe` on Windows).
Their source code is on the [Releases](https://github.com/diopterme/lapsewright/releases) page next to each version.
