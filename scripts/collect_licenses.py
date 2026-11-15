"""Collect license data for the C++ packages that ship in the engine.

Run by CI after a successful build. Writes vcpkg.tsv: one row per package
installed by vcpkg for the engine's target triplet.

Rust crates are covered by cargo-about (see about.toml), which resolves the
features actually enabled and therefore lists exactly the crates that ship.

Usage: python scripts/collect_licenses.py <output-dir>
"""
import glob
import json
import os
import sys


def vcpkg_report(path):
    rows = []
    pattern = os.path.join("build", "engine", "vcpkg_installed", "*", "share", "*", "vcpkg.spdx.json")
    for spdx_path in sorted(glob.glob(pattern)):
        with open(spdx_path, encoding="utf-8") as f:
            spdx = json.load(f)
        port = os.path.basename(os.path.dirname(spdx_path))
        triplet = spdx_path.replace("\\", "/").split("/")[-4]
        if triplet != "x64-windows-static-md":
            continue  # host tools, not shipped
        for package in spdx.get("packages", []):
            if package.get("SPDXID") == "SPDXRef-port":
                rows.append((port, package.get("versionInfo", ""),
                             package.get("licenseDeclared")
                             or package.get("licenseConcluded") or "(none declared)"))
    with open(path, "w", encoding="utf-8") as out:
        out.write("port\tversion\tlicense\n")
        for row in rows:
            out.write("\t".join(row) + "\n")
    return len(rows)


def main():
    out_dir = sys.argv[1]
    os.makedirs(out_dir, exist_ok=True)
    print("vcpkg ports:", vcpkg_report(os.path.join(out_dir, "vcpkg.tsv")))


if __name__ == "__main__":
    main()
