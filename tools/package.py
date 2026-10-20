"""Builds the MSIX package: release exe + manifest (version from Cargo.toml) + logos.

    python tools/package.py                          # dev identity, unsigned
    python tools/package.py --sign dev-cert.pfx      # signed for sideloading (see tools/dev_cert.ps1)
    python tools/package.py --revision 1             # 0.5.0.1, for testing updates
    python tools/package.py --store                  # Store identity (packaging/store.json), unsigned, for upload
    python tools/package.py --store --sign store-kit-cert.pfx --password dev
                                                     # also a signed copy for the certification kit only

Standard library only. Output: dist/screen-shifter-<version>-x64.msix, or -store.msix (upload) and
-store-kit.msix (certification kit, never uploaded) with --store.
"""

import argparse
import glob
import json
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
    ap.add_argument("--name")
    ap.add_argument("--publisher")
    ap.add_argument("--publisher-display")
    ap.add_argument("--display-name")
    ap.add_argument("--revision", type=int, default=0, help="fourth version part (the Store needs 0)")
    ap.add_argument("--sign", metavar="PFX", help="sign for sideloading with this certificate")
    ap.add_argument("--password", default="", help="PFX password")
    ap.add_argument("--store", action="store_true", help="the Store identity from packaging/store.json")
    a = ap.parse_args()

    identity = {"name": a.name, "publisher": a.publisher, "publisher_display": a.publisher_display, "display_name": a.display_name}
    if a.store:
        given = [k for k, v in identity.items() if v is not None]
        if given or a.revision:
            raise SystemExit(f"--store takes its identity from packaging/store.json and needs revision 0; drop {given or ['--revision']}")
        identity = json.load(open(os.path.join(ROOT, "packaging", "store.json"), encoding="utf-8-sig"))
        missing = [k for k in ("name", "publisher", "publisher_display", "display_name") if not identity.get(k) or identity[k].startswith("<")]
        if missing:
            raise SystemExit(f"packaging/store.json: fill in {missing} from Partner Center > the app > Product identity")
    else:
        defaults = {"name": "ScreenShifter.Dev", "publisher": "CN=ScreenShifterDev", "publisher_display": "screen-shifter (dev)", "display_name": "screen-shifter (dev)"}
        identity = {k: v if v is not None else defaults[k] for k, v in identity.items()}

    cargo = open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8").read()
    semver = re.search(r'^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"', cargo, re.M)
    version = ".".join(semver.groups()) + f".{a.revision}"

    run("cargo", "build", "--release")

    stage = os.path.join(ROOT, "stage")
    shutil.rmtree(stage, ignore_errors=True)
    shutil.copytree(os.path.join(ROOT, "packaging", "Assets"), os.path.join(stage, "Assets"))
    shutil.copy(os.path.join(ROOT, "target", "release", "screen-shifter.exe"), stage)
    manifest = open(os.path.join(ROOT, "packaging", "AppxManifest.xml"), encoding="utf-8").read()
    fill = {"NAME": identity["name"], "PUBLISHER": identity["publisher"], "PUBLISHER_DISPLAY": identity["publisher_display"],
            "DISPLAY_NAME": identity["display_name"], "VERSION": version}
    for key, value in fill.items():
        manifest = manifest.replace("{{" + key + "}}", value)
    with open(os.path.join(stage, "AppxManifest.xml"), "w", encoding="utf-8") as f:
        f.write(manifest)

    # resources.pri lets Windows pick the scale/targetsize/unplated logo variants
    run(sdk_tool("makepri.exe"), "new", "/pr", stage, "/cf", os.path.join(ROOT, "packaging", "priconfig.xml"),
        "/mn", os.path.join(stage, "AppxManifest.xml"), "/of", os.path.join(stage, "resources.pri"), "/o")

    os.makedirs(os.path.join(ROOT, "dist"), exist_ok=True)
    msix = os.path.join(ROOT, "dist", f"screen-shifter-{version}-x64{'-store' if a.store else ''}.msix")
    run(sdk_tool("makeappx.exe"), "pack", "/d", stage, "/p", msix, "/o")
    if a.sign:
        # The Store signs what is uploaded: with --store, only a separate copy is signed, for the kit.
        signed = msix.replace("-store.msix", "-store-kit.msix") if a.store else msix
        if signed != msix:
            shutil.copy(msix, signed)
        try:
            run(sdk_tool("signtool.exe"), "sign", "/fd", "SHA256", "/f", a.sign, "/p", a.password, signed)
        except subprocess.CalledProcessError:
            if signed != msix:
                os.remove(signed)  # never leave an unsigned "kit" copy (its subject must match the publisher)
            raise
        print(signed)
    print(msix)


if __name__ == "__main__":
    main()
