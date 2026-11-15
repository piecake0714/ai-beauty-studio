"""Assemble THIRD_PARTY_NOTICES.txt, the license notices shipped with the app.

Inputs, all produced by a normal build:
  --cargo-about   JSON from `cargo about generate --format json` for the app
  --vcpkg-share   <vcpkg_installed>/<triplet>/share (one folder per C++ package)
  --node-modules  apps/desktop/node_modules
  --npm-lock      apps/desktop/package-lock.json (decides which npm packages ship)
  --extra         "Title|path" of another shipped component's license text;
                  may be given several times (e.g. ONNX Runtime, DirectML)
  --out           output file

Fails (exit code 1) when any shipped component has no license text, so a
release can never go out with an incomplete notices file.
"""
import argparse
import glob
import json
import os
import sys

RULE = "=" * 78


def read_text(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read().strip()


def cargo_sections(path, problems):
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    sections = []
    for lic in data.get("licenses", []):
        users = sorted({
            f"{u['crate']['name']} {u['crate']['version']}"
            for u in lic.get("used_by", [])
            if "crate" in u
        })
        text = (lic.get("text") or "").strip()
        if not text:
            problems.append(f"Rust license {lic.get('id')} has no text")
        sections.append((f"{lic.get('name', lic.get('id'))}", "Used by: " + ", ".join(users), text))
    if not sections:
        problems.append("cargo-about produced no licenses")
    return sections


def vcpkg_sections(share_dir, problems):
    sections = []
    for port_dir in sorted(glob.glob(os.path.join(share_dir, "*"))):
        port = os.path.basename(port_dir)
        copyright_file = os.path.join(port_dir, "copyright")
        if not os.path.isfile(os.path.join(port_dir, "vcpkg.spdx.json")):
            continue  # not a package folder (e.g. CMake config only)
        if not os.path.isfile(copyright_file):
            problems.append(f"C++ package {port} has no copyright file")
            continue
        sections.append((port, "C++ library, statically linked into aibs_engine.dll", read_text(copyright_file)))
    if not sections:
        problems.append(f"no vcpkg packages found in {share_dir}")
    return sections


def npm_sections(node_modules, lock_path, problems):
    with open(lock_path, encoding="utf-8") as f:
        lock = json.load(f)
    sections = []
    for key, meta in sorted(lock.get("packages", {}).items()):
        if not key or meta.get("dev") or meta.get("devOptional"):
            continue
        name = key.split("node_modules/")[-1]
        package_dir = os.path.join(node_modules, *name.split("/"))
        candidates = sorted(
            p for p in glob.glob(os.path.join(package_dir, "*"))
            if os.path.basename(p).upper().startswith(("LICENSE", "LICENCE", "COPYING"))
        )
        if not candidates:
            problems.append(f"npm package {name} has no license file")
            continue
        sections.append((
            f"{name} {meta.get('version', '')}",
            f"JavaScript library, bundled into the user interface ({meta.get('license', 'license not declared')})",
            read_text(candidates[0]),
        ))
    return sections


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cargo-about", required=True)
    parser.add_argument("--vcpkg-share", required=True)
    parser.add_argument("--node-modules", required=True)
    parser.add_argument("--npm-lock", required=True)
    parser.add_argument("--extra", action="append", default=[])
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    problems = []
    extras = []
    for item in args.extra:
        title, _, path = item.partition("|")
        if not os.path.isfile(path):
            problems.append(f"license file for {title} not found: {path}")
            continue
        extras.append((title, "Runtime library shipped with the application", read_text(path)))
    groups = [
        ("Runtime libraries", extras),
        ("Rust libraries", cargo_sections(args.cargo_about, problems)),
        ("C++ libraries", vcpkg_sections(args.vcpkg_share, problems)),
        ("JavaScript libraries", npm_sections(args.node_modules, args.npm_lock, problems)),
    ]
    if problems:
        print("Notices are incomplete:", file=sys.stderr)
        for p in problems:
            print("  - " + p, file=sys.stderr)
        sys.exit(1)

    out = [
        "AI Beauty Studio - Third-party software notices",
        "",
        "AI Beauty Studio includes the following third-party software.",
        "Each component is listed with the license under which it is used.",
        "",
    ]
    for title, sections in groups:
        out += [RULE, title.upper(), RULE, ""]
        for name, note, text in sections:
            out += [f"--- {name}", note, "", text, ""]
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(out))
    counts = ", ".join(f"{title}: {len(s)}" for title, s in groups)
    print(f"wrote {args.out} ({counts})")


if __name__ == "__main__":
    main()
