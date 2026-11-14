#!/usr/bin/env python3
"""앱에 같이 넣는 ffmpeg(직접 빌드, ffmpeg.py) 와 그 Homebrew 부품들의 조건(라이선스)과 원본 코드.

    python3 crates/app/package/licenses.py      # 원본 코드 모으기 → GitHub Releases 에 올릴 tar

ffmpeg(GPL)를 나눠 주면 그 원본 코드도 같은 곳에 둬야 함 (notes/09 §9).
mac.py 는 write() 로 앱 안 Contents/Resources/licenses 에 조건 글을 넣음.
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import crates
import ffmpeg as ffmpeg_build
import mac

LICENSE_FILE = re.compile(r"^(COPYING|LICEN[CS]E|COPYRIGHT|NOTICE|.*GPL.*\.txt$)", re.I)
CELLAR = re.compile(r"/opt/homebrew/Cellar/([^/]+)/([^/]+)/")


def built():
    """(직접 빌드한 ffmpeg, 그 원본 tar.xz). 이미 빌드했으면 바로 돌려줌"""
    return ffmpeg_build.build(mac.FFMPEG_WORK)


def bundled():
    """{formula: Cellar 의 버전 폴더}: ffmpeg 가 부르는 dylib(앱에 같이 넣는 것)가 나온 brew 패키지"""
    found = {}
    for p in mac.closure([built()[0]]):
        m = CELLAR.match(str(p))
        if not m:
            sys.exit(f"brew 패키지가 아님: {p}")
        found[m.group(1)] = m.group(2)
    return dict(sorted(found.items()))


def info(names):
    data = json.loads(mac.out("brew", "info", "--json=v2", "--formula", *names))["formulae"]
    return {f["name"]: f for f in data}


def check_current(found, formulae):
    """원본은 지금 brew 공식 버전 것을 받음: 설치된 게 옛 버전이면 짝이 안 맞음"""
    old = []
    for name, version in found.items():
        f = formulae[name]
        current = f["versions"]["stable"] + (f"_{f['revision']}" if f["revision"] else "")
        if current != version:
            old.append(f"{name} {version} (지금 {current})")
    if old:
        sys.exit("옛 버전이 있음 → brew upgrade 하고 다시 빌드:\n  " + "\n  ".join(old))


def notice(found, formulae):
    conf = mac.out(built()[0], "-hide_banner", "-buildconf")
    rows = [f"{n:<16} {v:<12} {formulae[n].get('license') or '?':<40} {formulae[n]['homepage']}" for n, v in found.items()]
    return f"""Lapsewright bundles FFmpeg, built from the official source by crates/app/package/ffmpeg.py,
and the libraries it uses, as built by Homebrew for macOS (arm64).
Each part keeps its own license; license texts are in the folders next to this file.
The source code of every part, with our FFmpeg build script, the Homebrew build recipes (.rb) and patches, is published
next to the app on https://github.com/diopterme/lapsewright/releases (ffmpeg-sources-macos).

FFmpeg {ffmpeg_build.VERSION} — https://ffmpeg.org/ — GPL-3.0-or-later as configured below.
{conf}
{'name':<16} {'version':<12} {'license':<40} homepage
""" + "\n".join(rows) + "\n"


def write(dest):
    """dest 에 NOTICE.txt + 패키지마다 Cellar 에 있는 조건 글 + 우리 LICENSE"""
    found = bundled()
    formulae = info(found)
    check_current(found, formulae)
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True)
    shutil.copy2(mac.ROOT / "LICENSE", dest / "Lapsewright-LICENSE.txt")
    (dest / "NOTICE.txt").write_text(notice(found, formulae))
    (dest / "ffmpeg").mkdir()
    for f in ffmpeg_build.licenses(built()[0]):
        shutil.copy2(f, dest / "ffmpeg" / f.name)
    for name, version in found.items():
        cellar = mac.BREW / "Cellar" / name / version
        files = [f for f in cellar.iterdir() if f.is_file() and LICENSE_FILE.match(f.name)]
        (dest / name).mkdir()
        for f in files:
            shutil.copy2(f, dest / name / f.name)
        if not files:
            from_source(name, dest / name)
    return found


def from_source(name, dest):
    """brew 가 조건 글을 Cellar 에 안 넣은 패키지(leptonica 등): 원본 tar 맨 위 폴더에서 꺼냄"""
    subprocess.run(["brew", "fetch", "--build-from-source", "--formula", name], check=True, capture_output=True)
    tar = mac.out("brew", "--cache", "--build-from-source", "--formula", name).strip()
    members = [m for m in mac.out("tar", "-tf", tar).splitlines() if m.count("/") == 1 and re.search(r"licen[cs]e|copying", m, re.I)]
    if not members:
        sys.exit(f"조건 글을 못 찾음: {name} ({tar})")
    mac.sh("tar", "-xf", tar, "-C", dest, "--strip-components=1", *members)


def sources():
    """brew 가 빌드할 때 받는 것(원본·패치·.rb)을 새 캐시에 받아 tar 로"""
    version = json.loads((mac.TAURI / "tauri.conf.json").read_text())["version"]
    work = mac.ROOT / "target/release/ffmpeg-sources"
    top = work / f"lapsewright-{version}-ffmpeg-sources-macos"
    # cache 는 남김: 받다 끊기면 다시 돌릴 때 받은 것은 건너뜀
    shutil.rmtree(top, ignore_errors=True)
    found = write(top / "licenses")
    # 부품 목록마다 다른 캐시: 캐시를 통째로 묶으니, 예전에 받은 다른 부품 원본이 섞이면 안 됨
    key = hashlib.sha256(" ".join(f"{n}-{v}" for n, v in found.items()).encode()).hexdigest()[:12]
    cache = work / f"cache-{key}"
    env = {**os.environ, "HOMEBREW_CACHE": str(cache), "HOMEBREW_NO_AUTO_UPDATE": "1"}
    subprocess.run(["brew", "fetch", "--build-from-source", "--formula", *found], check=True, env=env)
    shutil.copytree(cache / "downloads", top / "downloads", copy_function=os.link, ignore=shutil.ignore_patterns("*.incomplete"))
    # git 으로 받는 부품(x264)은 downloads 가 아니라 cache/<이름>--git 에 받음 → 빌드한 시점의 코드만 tar 로
    for name, f in info(found).items():
        stable = f["urls"]["stable"]
        if stable["url"].endswith(".git"):
            ref = stable["revision"] or stable["tag"]
            mac.sh("git", "-C", cache / f"{name}--git", "archive", "--format=tar", f"--prefix={name}/", "-o", top / "downloads" / f"{name}-{ref}.tar", ref)
    (top / "ffmpeg").mkdir()
    for f in ffmpeg_build.sources(built()[1]):
        shutil.copy2(f, top / "ffmpeg" / f.name)
    # Rust 로 빌드하는 부품(rav1e): 빌드 때 crates.io 에서 받는 crate 원본도
    for name, f in info(found).items():
        if "rust" in f["build_dependencies"]:
            src = mac.out("brew", "--cache", "--build-from-source", "--formula", name).strip()
            crates.vendor(cache / "downloads" / Path(src).name, top / "crates")
    mac.RELEASE.mkdir(exist_ok=True)
    tar = mac.RELEASE / f"{top.name}.tar"
    mac.sh("tar", "-cf", tar, "-C", work, top.name)
    print(f"{tar} ({tar.stat().st_size / 1e6:.0f}MB) → Releases 에 앱과 같이 올림")


if __name__ == "__main__":
    sources()
