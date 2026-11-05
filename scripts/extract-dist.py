#!/usr/bin/env python3
"""Extract the Wuwa Tappo frontend from the official Windows installer into dist/.

The installer is a Tauri "installer shell" EXE that embeds an NSIS installer,
which contains WuwaTappo.exe, which embeds the frontend as a Tauri asset map
(brotli-compressed blobs referenced from a phf table in .rdata). This script
unpacks those layers so the vendor's own assets are never redistributed with
this repository.

Usage:
    python3 scripts/extract-dist.py [--dist] ~/Downloads/WuwaTappo-global_1.0.4_x64-setup.exe

Requires: 7-Zip (`7zz`, `brew install sevenzip`) and `pip install brotli`.
"""
import os
import pathlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile

try:
    import brotli
except ImportError:
    sys.exit("missing python module 'brotli' (pip3 install brotli)")

ROOT = pathlib.Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"


def sevenzip():
    for name in ("7zz", "7z"):
        if shutil.which(name):
            return name
    sys.exit("7-Zip not found (brew install sevenzip)")


def run(*args):
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def carve_inner_installer(shell_exe: pathlib.Path, work: pathlib.Path) -> pathlib.Path:
    """The installer shell keeps the NSIS payload as a raw PE blob in .rdata."""
    run(sevenzip(), "x", "-y", f"-o{work / 'shell'}", str(shell_exe))
    rdata = (work / "shell" / ".rdata").read_bytes()
    i = rdata.find(b"MZ\x90\x00")
    if i < 0:
        sys.exit("embedded NSIS installer not found in installer shell")
    inner = work / "inner.exe"
    inner.write_bytes(rdata[i:])
    return inner


def extract_app_exe(inner: pathlib.Path, work: pathlib.Path) -> pathlib.Path:
    out = work / "nsis"
    run(sevenzip(), "x", "-y", f"-o{out}", str(inner))
    # 1.0.0 named it WuwaTappo.exe; 1.0.3 stores it under an NSIS temp name.
    candidates = sorted(out.glob("*"), key=lambda p: p.stat().st_size, reverse=True)
    for c in candidates:
        if c.is_file() and c.read_bytes()[:2] == b"MZ" and c.stat().st_size > 20_000_000:
            return c
    sys.exit("WuwaTappo.exe not found inside NSIS payload")


def pe_sections(f: bytes):
    pe = struct.unpack_from("<I", f, 0x3C)[0]
    nsec = struct.unpack_from("<H", f, pe + 6)[0]
    optsz = struct.unpack_from("<H", f, pe + 20)[0]
    base = struct.unpack_from("<Q", f, pe + 24 + 24)[0]
    secs = []
    for i in range(nsec):
        o = pe + 24 + optsz + 40 * i
        name = f[o : o + 8].rstrip(b"\0").decode()
        vs, va, rs, ro = struct.unpack_from("<IIII", f, o + 8)
        secs.append((name, va, vs, ro, rs))
    return base, secs


def extract_assets(app_exe: pathlib.Path):
    f = app_exe.read_bytes()
    base, secs = pe_sections(f)
    _, rva, _, ro, rs = next(s for s in secs if s[0] == ".rdata")

    def va2off(v):
        r = v - base - rva
        return ro + r if 0 <= r < rs else None

    def off2va(o):
        return base + rva + (o - ro)

    rdata = f[ro : ro + rs]
    m = re.search(rb"/index\.html", rdata)
    if not m:
        sys.exit("asset map not found")
    key_off = ro + m.start()
    entry = ro + rdata.find(struct.pack("<QQ", off2va(key_off), len(m.group(0))))

    def read_entry(e):
        sp, sl, dp, dl = struct.unpack_from("<QQQQ", f, e)
        so, do = va2off(sp), va2off(dp)
        if so is None or do is None or sl > 200 or dl > 200_000_000:
            return None
        key = f[so : so + sl]
        if not key.startswith(b"/"):
            return None
        return key.decode(), do, dl

    # phf entries are contiguous 32-byte (key ptr, key len, data ptr, data len) records.
    while read_entry(entry - 32):
        entry -= 32
    entries = []
    while (ent := read_entry(entry)) is not None:
        entries.append(ent)
        entry += 32

    if DIST.exists():
        shutil.rmtree(DIST)
    for key, off, length in entries:
        target = DIST / key.lstrip("/")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(brotli.decompress(f[off : off + length]))
    return len(entries)


def extract_icons(work: pathlib.Path):
    """App icon (Kuro's artwork) → src-tauri/icons via `tauri icon`."""
    src = work / "shell" / ".rsrc" / "ICON"
    if not src.is_dir():
        print("warning: installer has no ICON resources; keeping existing icons")
        return
    best = max(src.iterdir(), key=lambda p: p.stat().st_size)
    icons = ROOT / "src-tauri" / "icons"
    icons.mkdir(parents=True, exist_ok=True)
    shutil.copy(best, icons / "icon.png")
    subprocess.run(
        ["npx", "--yes", "tauri", "icon", str(icons / "icon.png"), "-o", str(icons)],
        check=True, cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    for extra in ("android", "ios"):
        shutil.rmtree(icons / extra, ignore_errors=True)
    for p in icons.glob("Square*"):
        p.unlink()
    (icons / "StoreLogo.png").unlink(missing_ok=True)


def main():
    args = [a for a in sys.argv[1:] if a != "--dist"]
    if len(args) != 1:
        sys.exit(__doc__)
    installer = pathlib.Path(args[0]).expanduser()
    if not installer.is_file():
        sys.exit(f"not a file: {installer}")
    with tempfile.TemporaryDirectory() as tmp:
        work = pathlib.Path(tmp)
        inner = carve_inner_installer(installer, work)
        app_exe = extract_app_exe(inner, work)
        n = extract_assets(app_exe)
        extract_icons(work)
    print(f"extracted {n} assets to {DIST}")
    subprocess.run([sys.executable, str(ROOT / "scripts" / "patch-dist.py")], check=True)


if __name__ == "__main__":
    main()
