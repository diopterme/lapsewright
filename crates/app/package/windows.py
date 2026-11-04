#!/usr/bin/env python3
"""윈도우 설치 파일 준비: MSYS2 ffmpeg + 딸린 DLL + 조건 글 → crates/app/ffmpeg/, 원본 코드 → tar.

    (MSYS2 UCRT64 셸) python crates/app/package/windows.py
    그 뒤 (cmd) cargo tauri build   → tauri.windows.conf.json 이 crates/app/ffmpeg/ 를 앱 옆에 통째로 넣음

맥(mac.py + licenses.py)과 같은 방식: ffmpeg(GPL)를 나눠 주니 모든 부품의 원본 코드를 Releases 에 같이 둠.
원본은 MSYS2 가 패키지마다 올려 둔 것(repo.msys2.org/mingw/sources, 빌드 방법 PKGBUILD·패치 포함), 서명 확인함.
필요한 MSYS2 패키지: mingw-w64-ucrt-x86_64-{ffmpeg,ntldd,python} zstd git
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

import crates

ROOT = Path(__file__).resolve().parents[3]
TAURI = ROOT / "crates/app"
DEST = TAURI / "ffmpeg"
WORK = ROOT / "target/release/ffmpeg-sources"
PREFIX = "mingw-w64-ucrt-x86_64-"
MIRROR = "https://repo.msys2.org/mingw/sources/"
LICENSE_FILE = re.compile(r"^(COPYING|LICEN[CS]E|COPYRIGHT|NOTICE|.*GPL.*\.txt$)", re.I)
ARCHIVE = re.compile(r"\.(tar(\.\w+)?|tgz|zip)$")


def out(*args):
    return subprocess.run([str(a) for a in args], check=True, capture_output=True, text=True).stdout


def msys(path):
    """/ucrt64/bin/x → 파이썬이 여는 C:/.../msys64/ucrt64/bin/x"""
    return Path(out("cygpath", "-m", path).strip())


def dlls(ffmpeg):
    """ffmpeg.exe 가 (줄줄이) 부르는 MSYS2 DLL 전부. 윈도우 것(System32)은 뺌"""
    lines = out("ntldd", "-R", ffmpeg).splitlines()
    found = {Path(m.group(1).replace("\\", "/")) for l in lines if (m := re.search(r"=> (\S+) \(0x", l))}
    ucrt = msys("/ucrt64/bin")
    mine = sorted(p for p in found if p.parent.as_posix().lower() == ucrt.as_posix().lower())
    if not mine:
        sys.exit("DLL 을 못 찾음: ntldd 출력 확인")
    return mine


def packages(files):
    """{패키지 이름: (버전, base)}: files 가 나온 MSYS2 패키지"""
    owners = out("pacman", "-Qqo", *files).split()
    found = {}
    for name in sorted(set(owners)):
        version = out("pacman", "-Q", name).split()[1]
        desc = (msys("/var/lib/pacman/local") / f"{name}-{version}" / "desc").read_text()
        found[name] = (version, re.search(r"%BASE%\n(.+)", desc).group(1))
    return found


def fetch(base, version):
    """원본 묶음 받고 MSYS2 서명 확인. 받은 것은 cache 에 남김 (다시 돌리면 건너뜀)"""
    cache = WORK / "cache-windows"
    cache.mkdir(parents=True, exist_ok=True)
    src = cache / f"{base}-{version}.src.tar.zst"
    for f in (src, Path(f"{src}.sig")):
        if not f.exists():
            subprocess.run(["curl", "-fsSL", "--retry", "3", "-o", f"{f}.part", MIRROR + f.name], check=True)
            os.replace(f"{f}.part", f)
    # pacman-key --verify 는 관리자 권한을 요구 → 같은 MSYS2 키 모음으로 gpgv
    # 키 모음은 /etc/... 로 줌: gpgv 는 "C:/..." 를 URL 로 읽음
    if subprocess.run(["gpgv", "--keyring", "/etc/pacman.d/gnupg/pubring.gpg", f"{src}.sig", src], capture_output=True).returncode:
        sys.exit(f"서명이 안 맞음: {src}")
    return src


def unpack(src, tmp):
    """원본 묶음(.src.tar.zst) 풀기 → 그 안 파일들 (PKGBUILD, 패치, 원본 tar·zip, git 폴더)"""
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    with tarfile.open(src, "r:zst") as t:
        t.extractall(tmp, filter="data")
    return sorted(tmp.glob("*/*"))


def rust_crates(src, dest_dir):
    """PKGBUILD 가 cargo 를 쓰면 (rav1e·librsvg·libdovi): 빌드 때 받는 crate 원본도 받음"""
    with tarfile.open(src, "r:zst") as t:
        pkgbuild = t.extractfile(next(m for m in t.getmembers() if m.name.endswith("/PKGBUILD"))).read()
    if b"cargo" not in pkgbuild:
        return []
    files = unpack(src, WORK / "tmp" / src.name)
    return [t for f in files if ARCHIVE.search(f.name) and f.suffix != ".zip" and (t := crates.vendor(f, dest_dir))]


def licenses_from_source(src, dest, tmp):
    """패키지에 조건 글이 없을 때: 원본 묶음 안 원본(tar·zip·git)의 맨 위 폴더에서 꺼냄"""
    for f in unpack(src, tmp):
        if f.is_dir() and (f / "HEAD").exists():
            names = out("git", "--git-dir", f, "ls-tree", "--name-only", "HEAD").split()
            for n in filter(LICENSE_FILE.match, names):
                (dest / n).write_bytes(subprocess.run(["git", "--git-dir", f, "show", f"HEAD:{n}"], check=True, capture_output=True).stdout)
        elif ARCHIVE.search(f.name):
            extract_top(f, dest)
    if not any(dest.iterdir()):
        sys.exit(f"조건 글을 못 찾음: {src}")


def extract_top(archive, dest):
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as z:
            for n in z.namelist():
                if n.count("/") <= 1 and LICENSE_FILE.match(Path(n).name):
                    (dest / Path(n).name).write_bytes(z.read(n))
        return
    with tarfile.open(archive) as t:
        for m in t.getmembers():
            if m.isfile() and m.name.strip("./").count("/") <= 1 and LICENSE_FILE.match(Path(m.name).name):
                (dest / Path(m.name).name).write_bytes(t.extractfile(m).read())


def notice(ffmpeg, found):
    conf = out(ffmpeg, "-hide_banner", "-buildconf")
    rows = [f"{n.removeprefix(PREFIX):<16} {v:<28} {b}-{v}.src.tar.zst" for n, (v, b) in found.items()]
    return f"""Lapsewright bundles FFmpeg and the libraries it uses, as built by MSYS2 (UCRT64) for Windows x64.
Each part keeps its own license; license texts are in the folders next to this file.
The source code of every part, with the MSYS2 build recipes (PKGBUILD), patches and Rust crates, is published
next to the app on https://github.com/diopterme/lapsewright/releases (ffmpeg-sources-windows),
and also on {MIRROR}

FFmpeg {found[PREFIX + 'ffmpeg'][0]} — https://ffmpeg.org/ — GPL-3.0-or-later as configured below.
{conf}
{'name':<16} {'version':<28} source
""" + "\n".join(rows) + "\n"


def write_licenses(dest, ffmpeg, found, sources):
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True)
    shutil.copy2(ROOT / "LICENSE", dest / "Lapsewright-LICENSE.txt")
    (dest / "NOTICE.txt").write_text(notice(ffmpeg, found), encoding="utf-8")
    for name in found:
        short = name.removeprefix(PREFIX)
        (dest / short).mkdir()
        mine = [l for l in out("pacman", "-Qql", name).splitlines() if "/share/licenses/" in l and not l.endswith("/")]
        for l in mine:
            shutil.copy2(msys(l), dest / short / Path(l).name)
        if not mine:
            licenses_from_source(sources[name], dest / short, WORK / "tmp" / short)


def main():
    ffmpeg = msys("/ucrt64/bin/ffmpeg.exe")
    libs = dlls(ffmpeg)
    found = packages([ffmpeg, *libs])
    sources = {name: fetch(base, version) for name, (version, base) in found.items()}

    shutil.rmtree(DEST, ignore_errors=True)
    DEST.mkdir()
    for f in [ffmpeg, *libs]:
        shutil.copy2(f, DEST / f.name)
    write_licenses(DEST / "licenses", ffmpeg, found, sources)
    shutil.rmtree(WORK / "crates-windows", ignore_errors=True)
    vendored = [c for src in sorted(set(sources.values())) for c in rust_crates(src, WORK / "crates-windows")]

    version = json.loads((TAURI / "tauri.conf.json").read_text())["version"]
    top = f"lapsewright-{version}-ffmpeg-sources-windows"
    tar = WORK / f"{top}.tar"
    with tarfile.open(tar, "w") as t:
        t.add(DEST / "licenses", f"{top}/licenses")
        for src in sorted(set(sources.values())):
            t.add(src, f"{top}/sources/{src.name}")
            t.add(f"{src}.sig", f"{top}/sources/{src.name}.sig")
        for c in vendored:
            t.add(c, f"{top}/crates/{c.name}")
    shutil.rmtree(WORK / "tmp", ignore_errors=True)
    print(f"{DEST}: ffmpeg.exe + DLL {len(libs)}개 + licenses (부품 {len(found)}개)")
    print(f"{tar} ({tar.stat().st_size / 1e6:.0f}MB) → Releases 에 앱과 같이 올림")


if __name__ == "__main__":
    main()
