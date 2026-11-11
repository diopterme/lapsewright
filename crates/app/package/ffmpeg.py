"""앱에 넣을 ffmpeg 를 원본에서 직접 빌드 (mac.py · windows.py 가 부름, notes/09 §12).

Homebrew·MSYS2 ffmpeg 는 기능을 거의 다 켜서 안 쓰는 부품이 80개 넘게 딸려 옴.
여기선 ffmpeg 기본 기능은 다 두고, 바깥 부품은 앱이 쓰는 4개만 켬 (x264 · x265 · libjxl · zimg, 부품 자체는 Homebrew·MSYS2 것).
"""
import hashlib
import os
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

VERSION = "9.0.2"
URL = f"https://ffmpeg.org/releases/ffmpeg-{VERSION}.tar.xz"
SHA256 = "8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e"
# --disable-autodetect: 컴퓨터에 있으면 저절로 켜지는 부품(하드웨어 가속, 글꼴, 인터넷 암호화 …)을 다 끔 → 켤 것만 적음
CONFIGURE = [
    "--enable-gpl", "--enable-version3", "--disable-autodetect",
    "--enable-libx264", "--enable-libx265", "--enable-libjxl", "--enable-libzimg", "--enable-zlib",
    "--disable-ffplay", "--disable-ffprobe", "--disable-doc",
]
# 스레드도 autodetect 라서 따로 켬: 안 켜면 한 줄로만 돌아 느림
THREADS = {"darwin": "--enable-pthreads", "win32": "--enable-w32threads"}


def fetch(cache):
    """원본 tar.xz (받은 것은 cache 에 남김). 내용이 공식 sha256 과 다르면 멈춤"""
    cache.mkdir(parents=True, exist_ok=True)
    src = cache / Path(URL).name
    if not src.exists():
        subprocess.run(["curl", "-fsSL", "--retry", "3", "-o", f"{src}.part", URL], check=True)
        os.replace(f"{src}.part", src)
    if hashlib.sha256(src.read_bytes()).hexdigest() != SHA256:
        src.unlink()
        sys.exit(f"ffmpeg 원본 sha256 이 다름: {URL}")
    return src


def licenses(exe):
    """ffmpeg 원본 맨 위의 조건 글"""
    return [exe.parent / "LICENSE.md", *sorted(exe.parent.glob("COPYING.*"))]


def sources(src):
    """원본 묶음에 넣을 것: 원본 tar.xz + 이 빌드 설정 파일"""
    return [src, Path(__file__).resolve()]


def configure_args():
    return [*CONFIGURE, THREADS[sys.platform]]


def build(work):
    """work 에 풀고 빌드 → (ffmpeg 실행 파일, 원본 tar.xz). 같은 설정으로 이미 빌드했으면 건너뜀"""
    src = fetch(work / "cache")
    tree = work / f"ffmpeg-{VERSION}"
    exe = tree / ("ffmpeg.exe" if sys.platform == "win32" else "ffmpeg")
    stamp = tree / "lapsewright-configure.txt"
    args = configure_args()
    if exe.exists() and stamp.exists() and stamp.read_text() == " ".join(args):
        return exe, src
    shutil.rmtree(tree, ignore_errors=True)
    with tarfile.open(src) as t:
        t.extractall(work, filter="data")
    subprocess.run(["sh", "./configure", *args], check=True, cwd=tree)
    subprocess.run(["make", f"-j{os.cpu_count() or 4}"], check=True, cwd=tree)
    stamp.write_text(" ".join(args))
    return exe, src
