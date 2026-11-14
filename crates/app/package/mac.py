#!/usr/bin/env python3
"""맥 설치 파일(.dmg): 앱 빌드 → ffmpeg 빌드(ffmpeg.py, 부품은 Homebrew 것) 를 앱 안에 → Developer ID 서명 → dmg → 애플 공증.

    python3 crates/app/package/mac.py [--no-build] [--no-notarize]

ffmpeg 는 앱이 찾는 자리(Contents/MacOS/ffmpeg)에, 딸린 dylib 는 Contents/Frameworks 로 모아 경로를 앱 안으로 바꿈.
조건 글은 Contents/Resources/licenses (licenses.py). 원본 코드 묶음은 licenses.py 로 따로.
공증 암호는 키체인의 `notary` (xcrun notarytool store-credentials 로 넣은 것). --no-notarize = 서명까지만 (내 맥 시험용).
"""
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import ffmpeg as ffmpeg_build

ROOT = Path(__file__).resolve().parents[3]
TAURI = ROOT / "crates/app"
APP = ROOT / "target/release/bundle/macos/Lapsewright.app"
FFMPEG_WORK = ROOT / "target/release/ffmpeg-build"
# Finished files only (dmg, setup.exe, source tars); everything else stays in target/ (not synced).
RELEASE = ROOT / "release"
BREW = Path("/opt/homebrew")
SYSTEM = ("/usr/lib/", "/System/")
IDENTITY = "Developer ID Application: yeongsu son (H355569U4M)"
NOTARY_PROFILE = "notary"


def sh(*args, cwd=None):
    subprocess.run([str(a) for a in args], check=True, cwd=cwd)


def out(*args):
    return subprocess.run([str(a) for a in args], check=True, capture_output=True, text=True).stdout


def deps(p):
    return [l.strip().split(" (")[0] for l in out("otool", "-L", p).splitlines()[1:]]


def resolve(p, d):
    """p 가 부르는 d 의 실제 파일(@rpath·@loader_path 풀기), 시스템 것은 None"""
    if d.startswith(SYSTEM):
        return None
    if d.startswith("@loader_path/"):
        return Path(os.path.realpath(Path(p).parent / d[len("@loader_path/"):]))
    if d.startswith("@rpath/"):
        for rp in re.findall(r"cmd LC_RPATH\n.*\n\s+path (\S+)", out("otool", "-l", p)):
            c = Path(rp.replace("@loader_path", str(Path(p).parent)).replace("@executable_path", str(Path(p).parent))) / d[7:]
            if c.exists():
                return Path(os.path.realpath(c))
        sys.exit(f"못 찾음: {p} 가 부르는 {d}")
    if not Path(d).exists():
        sys.exit(f"못 찾음: {p} 가 부르는 {d}")
    return Path(os.path.realpath(d))


def closure(bins):
    """bins 가 (줄줄이) 부르는 brew dylib 전부"""
    seen, todo = {}, list(bins)
    while todo:
        p = todo.pop()
        for d in deps(p):
            r = resolve(p, d)
            if r and r != Path(os.path.realpath(p)) and r not in seen:
                seen[r] = 1
                todo.append(r)
    return list(seen)


def relink(f, orig, prefix):
    """복사본 f 가 부르는 비시스템 dylib 를 prefix/이름 으로(@rpath 등은 원래 자리 orig 기준으로 풂)"""
    os.chmod(f, 0o755)
    args = []
    for d in deps(f):
        r = resolve(orig, d)
        if r and r != Path(os.path.realpath(orig)):
            args += ["-change", d, f"{prefix}/{r.name}"]
    if args:
        subprocess.run(["install_name_tool", *args, str(f)], check=True, stderr=subprocess.DEVNULL)  # 서명 깨짐 경고 = 뒤에서 다시 서명


def fill(app):
    c = app / "Contents"
    fw = c / "Frameworks"
    shutil.rmtree(fw, ignore_errors=True)
    fw.mkdir(parents=True)
    src, _ = ffmpeg_build.build(FFMPEG_WORK)
    dst = c / "MacOS/ffmpeg"
    shutil.copy2(src, dst)
    for lib in closure([src]):
        shutil.copy2(lib, fw / lib.name)
        relink(fw / lib.name, lib, "@loader_path")
        subprocess.run(["install_name_tool", "-id", f"@loader_path/{lib.name}", str(fw / lib.name)], check=True, stderr=subprocess.DEVNULL)
    relink(dst, src, "@loader_path/../Frameworks")
    import licenses  # licenses 도 mac 을 부르니 여기서

    licenses.write(c / "Resources/licenses")
    # 안쪽부터: 앱 서명은 안에 든 것들의 서명을 묶음
    for f in [*fw.iterdir(), dst]:
        sign(f)
    sign(app)


def sign(f):
    """공증 조건: Developer ID + hardened runtime + 시간 도장"""
    sh("codesign", "--force", "--options", "runtime", "--timestamp", "-s", IDENTITY, f)


def check(app):
    """brew 경로가 남았는지 + ffmpeg 가 앱 안 것으로 도는지"""
    c = app / "Contents"
    left = [f"{f}: {d}" for f in [*c.joinpath("MacOS").iterdir(), *c.joinpath("Frameworks").iterdir()]
            for d in deps(f) if d.startswith(str(BREW))]
    if left:
        sys.exit("brew 경로가 남음:\n" + "\n".join(left))
    r = subprocess.run([c / "MacOS/ffmpeg", "-version"], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"ffmpeg 안 돎: {r.stderr[-300:]}")
    print(f"  ffmpeg: {r.stdout.splitlines()[0][:70]}")
    sh("codesign", "--verify", "--deep", "--strict", app)


def dmg(app):
    version = json.loads((TAURI / "tauri.conf.json").read_text())["version"]
    RELEASE.mkdir(exist_ok=True)
    dest = RELEASE / f"Lapsewright_{version}_aarch64.dmg"
    stage = ROOT / "target/release/bundle/dmg-stage"
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir(parents=True)
    sh("ditto", app, stage / app.name)
    (stage / "Applications").symlink_to("/Applications")
    dest.unlink(missing_ok=True)
    sh("hdiutil", "create", "-volname", "Lapsewright", "-srcfolder", stage, "-format", "UDZO", "-ov", dest)
    shutil.rmtree(stage)
    sh("codesign", "--force", "--timestamp", "-s", IDENTITY, dest)
    print(f"{dest} ({dest.stat().st_size / 1e6:.0f}MB)")
    return dest


def notarize(dmg):
    """애플에 올려 검사(몇 분) → 통과 표를 dmg 에 붙임: 인터넷 없이도 처음 열 때 경고 없음"""
    run = subprocess.run(["xcrun", "notarytool", "submit", str(dmg), "--keychain-profile", NOTARY_PROFILE, "--wait", "--output-format", "json"],
                         capture_output=True, text=True)
    try:
        r = json.loads(run.stdout)
    except json.JSONDecodeError:
        sys.exit(f"공증에 못 올림: {run.stderr.strip() or run.stdout.strip()}")
    if r.get("status") != "Accepted":
        print(out("xcrun", "notarytool", "log", r["id"], "--keychain-profile", NOTARY_PROFILE))
        sys.exit(f"공증 실패: {r.get('status')} ({r['id']})")
    sh("xcrun", "stapler", "staple", dmg)
    sh("spctl", "--assess", "--type", "open", "--context", "context:primary-signature", "-vv", dmg)


if __name__ == "__main__":
    if "--no-build" not in sys.argv:
        sh("cargo", "tauri", "build", cwd=TAURI)
    fill(APP)
    check(APP)
    image = dmg(APP)
    # The app itself next to the dmg, to run without mounting it.
    shutil.rmtree(RELEASE / APP.name, ignore_errors=True)
    sh("ditto", APP, RELEASE / APP.name)
    if "--no-notarize" not in sys.argv:
        notarize(image)
