"""Collect license data for everything that ships in the application.

Run by CI after a successful build. Writes tab-separated reports:
  cargo.tsv  - Rust crates reachable from the app through normal
               (non-dev, non-build) dependencies on Windows
  vcpkg.tsv  - C++ packages installed by vcpkg for the engine

Usage: python scripts/collect_licenses.py <output-dir>
"""
import glob
import json
import os
import subprocess
import sys

ROOT_PACKAGE = "ai-beauty-studio"
TARGET = "x86_64-pc-windows-msvc"


def cargo_report(path):
    raw = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--filter-platform", TARGET],
        check=True, capture_output=True, text=True, encoding="utf-8",
    ).stdout
    meta = json.loads(raw)
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == ROOT_PACKAGE)

    shipped, stack = set(), [root]
    while stack:
        current = stack.pop()
        if current in shipped:
            continue
        shipped.add(current)
        for dep in nodes[current]["deps"]:
            # kind None = normal dependency (linked into the binary)
            if any(k.get("kind") is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])

    rows = sorted(
        (packages[i]["name"], packages[i]["version"],
         packages[i].get("license") or "(none declared)",
         packages[i].get("source") or "workspace")
        for i in shipped
    )
    with open(path, "w", encoding="utf-8") as out:
        out.write("name\tversion\tlicense\tsource\n")
        for row in rows:
            out.write("\t".join(row) + "\n")
    return len(rows)


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
    print("cargo crates:", cargo_report(os.path.join(out_dir, "cargo.tsv")))
    print("vcpkg ports:", vcpkg_report(os.path.join(out_dir, "vcpkg.tsv")))


if __name__ == "__main__":
    main()
