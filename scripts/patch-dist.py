#!/usr/bin/env python3
"""Apply the macOS-port patches to the extracted frontend bundle (dist/).

Each patch is an exact-string replacement that must match exactly once, so a
different upstream build fails loudly instead of silently shipping unpatched.
"""
import pathlib, sys

DIST = pathlib.Path(__file__).resolve().parent.parent / "dist" / "assets"

PATCHES = [
    # listen$1 (main bundle, hashed filename): tell the backend when a
    # listener is registered so window-open events (login-modal-open,
    # cat-app-update-available, ...) can be delivered exactly once after the
    # page has mounted.
    (
        "async function listen$1(",
        'handler:transformCallback$1(t)}).then(r=>async()=>_unlisten$1(e,r))}',
        'handler:transformCallback$1(t)}).then(r=>(invoke$1("listener_ready",{event:e}).catch(()=>{}),async()=>_unlisten$1(e,r)))}',
    ),
]

def find_bundle(marker: str) -> pathlib.Path:
    hits = [p for p in DIST.glob("*.js") if marker in p.read_text(encoding="utf-8")]
    if len(hits) != 1:
        print(f"expected exactly one bundle containing {marker!r}, found {len(hits)}", file=sys.stderr)
        sys.exit(1)
    return hits[0]

def main():
    for marker, old, new in PATCHES:
        p = find_bundle(marker)
        name = p.name
        s = p.read_text(encoding="utf-8")
        if new in s:
            print(f"{name}: already patched")
            continue
        n = s.count(old)
        if n != 1:
            print(f"{name}: expected 1 match, found {n}", file=sys.stderr)
            sys.exit(1)
        p.write_text(s.replace(old, new), encoding="utf-8")
        print(f"{name}: patched")

if __name__ == "__main__":
    main()
