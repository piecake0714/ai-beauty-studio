"""Download third-party model files listed in models/registry.json.

Third-party weights are not stored in Git. Each registry entry with a
"url" is downloaded to its "file" path and checked against its pinned
"sha256"; a mismatch deletes the file and fails. The entry's license file
(the upstream LICENSE text) is fetched the same way from "licenseUrl".

    python scripts/fetch_models.py           # download and verify
    python scripts/fetch_models.py --print   # also print hashes

An entry whose sha256 is "unpinned" is downloaded and its hash printed so
it can be pinned; the app refuses to load it until then.
"""

import hashlib
import json
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MODELS = ROOT / "models"


def download(url: str, target: Path) -> bytes:
    target.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(url, timeout=120) as response:
        data = response.read()
    target.write_bytes(data)
    return data


def main() -> int:
    show = "--print" in sys.argv
    registry = json.loads((MODELS / "registry.json").read_text(encoding="utf-8"))
    failures = 0
    for entry in registry["models"]:
        url = entry.get("url")
        if not url:
            continue
        target = MODELS / entry["file"]
        expected = entry["sha256"].lower()
        if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest() == expected:
            data = target.read_bytes()
            print(f"{entry['id']}: already present")
        else:
            data = download(url, target)
            print(f"{entry['id']}: downloaded {len(data)} bytes from {url}")
        actual = hashlib.sha256(data).hexdigest()
        if show or expected == "unpinned":
            print(f"{entry['id']}: sha256 {actual}")
        if expected != "unpinned" and actual != expected:
            print(f"{entry['id']}: HASH MISMATCH, expected {expected}", file=sys.stderr)
            target.unlink()
            failures += 1
        upstream = entry.get("licenseUrl")
        if upstream:
            text_path = MODELS / "licenses" / f"{entry['id']}.LICENSE.txt"
            text = download(upstream, text_path)
            print(f"{entry['id']}: license text {len(text)} bytes, sha256 {hashlib.sha256(text).hexdigest()}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
