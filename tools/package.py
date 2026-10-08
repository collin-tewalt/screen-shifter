"""Builds the MSIX package: release exe + manifest (version from Cargo.toml) + logos.

    python tools/package.py                          # dev identity, unsigned
    python tools/package.py --sign dev-cert.pfx      # signed for sideloading (see tools/dev_cert.ps1)
    python tools/package.py --revision 1             # 0.5.0.1, for testing updates
    python tools/package.py --name <Store name> --publisher "CN=<Store id>" --publisher-display "<you>"

Standard library only. Output: dist/screen-shifter-<version>-x64.msix
"""

import argparse
import glob
import os
import re
import shutil
import subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def sdk_tool(name):
    """Newest Windows SDK copy of makeappx/makepri/signtool."""
    hits = glob.glob(rf"C:\Program Files (x86)\Windows Kits\10\bin\10.*\x64\{name}")
    if not hits:
        raise SystemExit(f"{name} not found: install the Windows SDK")
    return max(hits, key=lambda p: [int(x) for x in p.split(os.sep)[-3].split(".")])


def run(*args):
    print(">", " ".join(args))
    subprocess.run(args, check=True, cwd=ROOT)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--name", default="ScreenShifter.Dev")
    ap.add_argument("--publisher", default="CN=ScreenShifterDev")
    ap.add_argument("--publisher-display", default="screen-shifter (dev)")
    ap.add_argument("--revision", type=int, default=0, help="fourth version part (the Store needs 0)")
    ap.add_argument("--sign", metavar="PFX", help="sign for sideloading with this certificate")
    ap.add_argument("--password", default="", help="PFX password")
    a = ap.parse_args()

    cargo = open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8").read()
    semver = re.search(r'^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"', cargo, re.M)
    version = ".".join(semver.groups()) + f".{a.revision}"

    run("cargo", "build", "--release")

    stage = os.path.join(ROOT, "stage")
    shutil.rmtree(stage, ignore_errors=True)
    shutil.copytree(os.path.join(ROOT, "packaging", "Assets"), os.path.join(stage, "Assets"))
    shutil.copy(os.path.join(ROOT, "target", "release", "screen-shifter.exe"), stage)
    manifest = open(os.path.join(ROOT, "packaging", "AppxManifest.xml"), encoding="utf-8").read()
    for key, value in {"NAME": a.name, "PUBLISHER": a.publisher, "PUBLISHER_DISPLAY": a.publisher_display, "VERSION": version}.items():
        manifest = manifest.replace("{{" + key + "}}", value)
    with open(os.path.join(stage, "AppxManifest.xml"), "w", encoding="utf-8") as f:
        f.write(manifest)

    # resources.pri lets Windows pick the scale/targetsize/unplated logo variants
    run(sdk_tool("makepri.exe"), "new", "/pr", stage, "/cf", os.path.join(ROOT, "packaging", "priconfig.xml"),
        "/mn", os.path.join(stage, "AppxManifest.xml"), "/of", os.path.join(stage, "resources.pri"), "/o")

    os.makedirs(os.path.join(ROOT, "dist"), exist_ok=True)
    msix = os.path.join(ROOT, "dist", f"screen-shifter-{version}-x64.msix")
    run(sdk_tool("makeappx.exe"), "pack", "/d", stage, "/p", msix, "/o")
    if a.sign:
        run(sdk_tool("signtool.exe"), "sign", "/fd", "SHA256", "/f", a.sign, "/p", a.password, msix)
    print(msix)


if __name__ == "__main__":
    main()
