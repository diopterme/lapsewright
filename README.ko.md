<p align="center">
  <img src="crates/app/icons/128x128@2x.png" width="128" height="128" alt="Lapsewright 아이콘">
</p>

<h1 align="center">Lapsewright</h1>

<p align="center">
  사진 폴더를 타임랩스 영상으로 만드는 앱. ffmpeg 명령어를 칠 필요가 없습니다.
</p>

<p align="center">
  <a href="https://github.com/diopterme/lapsewright/releases/latest"><img src="https://img.shields.io/github/v/release/diopterme/lapsewright" alt="최신 버전"></a>
  <a href="https://github.com/diopterme/lapsewright/releases"><img src="https://img.shields.io/github/downloads/diopterme/lapsewright/total" alt="내려받기 수"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/diopterme/lapsewright" alt="라이선스: GPL-3.0"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey" alt="플랫폼: macOS | Windows">
</p>

<p align="center">
  <a href="README.md">English</a> | 한국어
</p>

## 기능

- **사진:** JPG, PNG, JPEG XL, TIFF (16비트 포함). 한 폴더엔 한 가지 형식만
- **영상:** H.264, HEVC, ProRes / SDR, HLG, HDR (PQ)
- **설정:** 크기, 초당 장 수, 화질, 모션 블러. 자를 위치는 화면에서 끌어서 정함
- **메타데이터:** 사진의 찍은 날짜, 카메라, 렌즈, 위치, 저작권을 영상에 그대로 넣음
- **대기열:** 여러 폴더를 넣고, 하나 끝날 때마다 알림 (맥)
- **언어:** 한국어·영어 (맥/윈도우 언어를 따라감)

## 설치

[Releases](https://github.com/diopterme/lapsewright/releases/latest) 에서 최신 버전을 받으세요. ffmpeg 가 들어 있습니다.

| 플랫폼 | 파일 | 안내 |
| --- | --- | --- |
| macOS (애플 실리콘) | `Lapsewright_<버전>_aarch64.dmg` | 열고 Lapsewright 를 응용 프로그램 폴더로 끌어 넣으세요. 애플 서명·공증을 받은 앱입니다. |
| Windows 10/11 (64비트) | `Lapsewright_<버전>_x64-setup.exe` | 서명이 없습니다. "Windows의 PC 보호" 창이 뜨면 **추가 정보** → **실행** 을 누르세요. |

## 소스로 빌드하기

### 필요한 것

- [Rust](https://rustup.rs) (edition 2024)
- [Node.js](https://nodejs.org) 22
- Tauri CLI 2: `cargo install tauri-cli --version "^2"`
- ffmpeg 9.0 이상 (맥: `brew install ffmpeg-full`)

### 실행

```sh
(cd crates/app/ui && npm install)
(cd crates/app && LAPSEWRIGHT_FFMPEG=/opt/homebrew/bin/ffmpeg cargo tauri dev)
```

개발 중엔 `LAPSEWRIGHT_FFMPEG` 에 적은 ffmpeg 를 씁니다.

### 테스트

```sh
cargo test --workspace
(cd crates/app/ui && npm run check && node --test src/lib/)
```

### 배포 파일 만들기

**맥:** `python3 crates/app/package/mac.py` 가 `.dmg` 를 만듭니다. Homebrew 의 ffmpeg 와 부품을 앱 안에 넣고, 사용 조건 글을 넣은 뒤 서명·공증을 합니다.
`python3 crates/app/package/licenses.py` 는 그 부품들의 원본 코드를 모아 Releases 에 올릴 파일로 만듭니다.

**윈도우:** [MSYS2](https://www.msys2.org) UCRT64 셸에서 `python crates/app/package/windows.py` 를 실행합니다. MSYS2 의 ffmpeg 와 DLL, 조건 글을 `crates/app/ffmpeg/` 에 모으고 원본 코드도 모읍니다.
그다음 `crates/app` 에서 `cargo tauri build` 를 합니다.

## 명령줄

`crates/cli` 는 창 없이 같은 기능을 씁니다:

```sh
cargo build -p lapsewright-cli
target/debug/lapsewright --ffmpeg /opt/homebrew/bin/ffmpeg convert <사진 폴더>
target/debug/lapsewright --help
```

## 개인정보

Lapsewright 는 인터넷 없이 동작합니다. 밖으로 나가는 요청은 장소 검색 하나뿐이고, 입력한 장소 이름만 [OpenStreetMap Nominatim](https://nominatim.openstreetmap.org) 으로 보냅니다.

## 후원

Lapsewright 는 무료입니다. 도움이 됐다면 [GitHub 에서 후원](https://github.com/sponsors/diopterme)할 수 있습니다.

## 라이선스

Lapsewright 는 [GNU GPL v3.0 이상](LICENSE) 조건의 자유 소프트웨어입니다.
누구나 쓰고, 고치고, 나눌 수 있습니다. 고친 것을 나눌 때는 그 코드도 공개해야 합니다.

앱에는 [FFmpeg](https://ffmpeg.org) (GPLv3 빌드) 와 FFmpeg 가 쓰는 부품들이 들어 있고, 부품마다 자기 조건이 있습니다.
조건 글은 앱 안에 있습니다 (맥: `Lapsewright.app/Contents/Resources/licenses`, 윈도우: `Lapsewright.exe` 옆 `licenses` 폴더).
원본 코드는 [Releases](https://github.com/diopterme/lapsewright/releases) 의 버전마다 같이 올려 둡니다.
