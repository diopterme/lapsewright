"""Rust 로 만든 부품(rav1e 등)은 빌드 때 crates.io 에서 부품을 더 받음 → 그 원본도 같이 둠 (licenses.py · windows.py)."""
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path


def vendor(archive, dest_dir):
    """archive 맨 위 폴더에 Cargo.lock 이 있고 vendor/ 가 없으면: 그 Cargo.lock 그대로 받은 crate 들을 tar 로. 아니면 None"""
    with tarfile.open(archive) as t:
        names = [n.removeprefix("./") for n in t.getnames()]
    locks = [n for n in names if n.count("/") == 1 and n.endswith("/Cargo.lock")]
    if not locks or any(n.count("/") >= 1 and n.split("/")[1] == "vendor" for n in names):
        return None
    top = locks[0].split("/")[0]
    Path(dest_dir).mkdir(parents=True, exist_ok=True)
    out = Path(dest_dir) / f"{top}-crates.tar.gz"
    # 우리 저장소 밖에서 풂: 안에서 풀면 cargo 가 우리 Cargo.toml 을 자기 workspace 로 잡음
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        with tarfile.open(archive) as t:
            t.extractall(work, filter="data")
        r = subprocess.run([cargo(), "vendor", "--locked", "--versioned-dirs", "--manifest-path", work / top / "Cargo.toml", work / "vendor"],
                           capture_output=True, text=True)
        if r.returncode:
            raise SystemExit(f"cargo vendor 실패: {archive}\n{r.stderr}")
        with tarfile.open(out, "w:gz") as t:
            t.add(work / top / "Cargo.lock", f"{top}-crates/Cargo.lock")
            t.add(work / "vendor", f"{top}-crates/vendor")
    return out


def cargo():
    """MSYS2 셸은 윈도우 PATH 를 안 물려받음 → rustup 기본 자리도 봄"""
    found = shutil.which("cargo") or shutil.which(str(Path.home() / ".cargo/bin/cargo"))
    if not found:
        raise SystemExit("cargo 가 없음: Rust 부품의 crate 원본을 받으려면 필요")
    return found
