"""Makes the installer for players, installer_out\\Cybertron Escalation Setup
<version>.exe ("make installer.bat"): a Setup that installs the demo ready to
play, with everything its game-files build needs, for players who'd rather not
install Rust, Python, ffmpeg and UE Viewer themselves.

usage: python tools\\make_installer.py [--iscc <ISCC.exe>] [--umodel <umodel_64.exe>]
                                       [--skip-build]

Run it on the PC the demo is made on (Windows, with what the README's sections 1
and 3 install: Rust and its C++ tools, Python from python.org with numpy, Pillow
and fontTools, and UE Viewer: --umodel, or where extract_game.py finds it, in
tools\\umodel or on the PATH), with Inno Setup 6.3 or newer
installed (https://jrsoftware.org/isdl.php, or: winget install JRSoftware.InnoSetup).
It:
1. builds foc_esc.exe, optimized, with the C runtime built in (so it needs
   nothing else installed), into target\\installer, with Setup's version in it
   (FOC_ESC_VERSION: the main menu shows it at its bottom left, and File
   Explorer's Properties > Details as the program's version); --skip-build uses
   the one built last time, and its version (target\\installer\\version.txt);
2. gathers what Setup installs into installer_work\\stage: foc_esc.exe, the
   demo's own shaders (assets\\shaders), its icon, the asset build's scripts
   (tools\\, with tools\\installer\\builder.py, the window that runs the build) and
   UE Viewer (tools\\umodel), and a Python of their own (python\\: a copy of this
   PC's, without its tests or packages, with numpy, Pillow and fontTools as this
   PC has them), and checks that Python runs the build's scripts and the
   builder's window (UE Viewer goes into the installed copy's tools\\umodel);
3. notes the build tools' id in tools\\installer\\pipeline.txt (a hash of the
   build's scripts and UE Viewer): an installed copy whose game files were built
   with other tools builds them again;
4. compiles tools\\installer\\foc_esc.iss with Inno Setup into installer_out.

Setup doesn't hold any of the game's files: the players' builds make them from
their own copies. ffmpeg (GPL) isn't in it either: the builder downloads it.
"""
import argparse
import datetime
import hashlib
import importlib.metadata
import os
import re
import shutil
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
STAGE = os.path.join(ROOT, "installer_work", "stage")
OUT = os.path.join(ROOT, "installer_out")
ISS = os.path.join(HERE, "installer", "foc_esc.iss")
TARGET = "x86_64-pc-windows-msvc"

# The build's Python packages (extract_game.py's check_python).
PACKAGES = ["numpy", "pillow", "fonttools"]
# What the bundled Python leaves out of this PC's: its tests, IDLE, the docs and
# the files for building extensions, and its installed packages (the build's are
# installed afresh).
PYTHON_SKIP_DIRS = {os.path.normcase(p) for p in [
    "Lib/test", "Lib/idlelib", "Lib/turtledemo", "Lib/site-packages", "Lib/ensurepip",
    "Lib/venv", "Doc", "Tools", "Scripts", "include", "libs", "tcl/tix8.4.3"]}
PYTHON_SKIP_FILES = {"NEWS.txt"}
# The C runtime's DLLs: foc_esc.exe mustn't need them (built with it inside).
CRT_DLLS = re.compile(r"^(vcruntime\d+(_\d)?|msvcp\d+(_\w+)?|ucrtbase|api-ms-win-crt-.*)\.dll$", re.I)
# The tools the installed copy doesn't need: the tests, the installer's own files.
TOOLS_SKIP = {"__pycache__", "umodel", "ffmpeg"}
# (Tools for making the demo, not its game files: not installed, so not part of
# the build tools' id either.)
TOOLS_SKIP_FILES = {"make_installer.py", "make_icon.py", "compare_assets.py"}
INSTALLER_SKIP = {"test_builder.py", "foc_esc.iss", "licenses", "__pycache__"}


def fail(msg):
    print("\n! " + msg)
    sys.exit(1)


def step(msg):
    print("\n== " + msg, flush=True)


# ---------------------------------------------------------------------------
# Finding Inno Setup
# ---------------------------------------------------------------------------

def find_iscc(given):
    if given:
        return given if os.path.isfile(given) else None
    found = shutil.which("ISCC") or shutil.which("iscc")
    if found:
        return found
    places = []
    if os.name == "nt":
        import winreg
        for hive, key in [
                (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Inno Setup 6_is1"),
                (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Inno Setup 6_is1"),
                (winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Inno Setup 6_is1")]:
            try:
                with winreg.OpenKey(hive, key) as k:
                    places.append(winreg.QueryValueEx(k, "InstallLocation")[0])
            except OSError:
                pass
    for env in ["ProgramFiles(x86)", "ProgramFiles"]:
        if os.environ.get(env):
            places.append(os.path.join(os.environ[env], "Inno Setup 6"))
    if os.environ.get("LOCALAPPDATA"):
        places.append(os.path.join(os.environ["LOCALAPPDATA"], "Programs", "Inno Setup 6"))
    for p in places:
        exe = os.path.join(p, "ISCC.exe")
        if os.path.isfile(exe):
            return exe
    return None


# ---------------------------------------------------------------------------
# What a Windows program needs (its imports)
# ---------------------------------------------------------------------------

def pe_imports(path):
    """The DLLs a Windows program (PE) file imports, from its import table."""
    with open(path, "rb") as f:
        data = f.read()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError(f"{path} isn't a Windows program")
    sections_n = struct.unpack_from("<H", data, pe + 6)[0]
    opt_size = struct.unpack_from("<H", data, pe + 20)[0]
    opt = pe + 24
    magic = struct.unpack_from("<H", data, opt)[0]
    dirs = opt + (112 if magic == 0x20B else 96)
    imports_rva = struct.unpack_from("<I", data, dirs + 8)[0]
    sections = [struct.unpack_from("<8sIIII", data, opt + opt_size + i * 40) for i in range(sections_n)]

    def offset(rva):
        for _, vsize, va, raw_size, raw_ptr in sections:
            if va <= rva < va + max(vsize, raw_size):
                return raw_ptr + rva - va
        raise ValueError(f"RVA {rva:#x} in no section")

    out = []
    if not imports_rva:
        return out
    o = offset(imports_rva)
    while True:
        _, _, _, name_rva, first_thunk = struct.unpack_from("<IIIII", data, o)
        if name_rva == 0 and first_thunk == 0:
            break
        n = offset(name_rva)
        out.append(data[n:data.index(b"\0", n)].decode("ascii", "replace"))
        o += 20
    return out


# ---------------------------------------------------------------------------
# The steps
# ---------------------------------------------------------------------------

def build_exe(skip):
    """foc_esc.exe and its version (Setup's too): built now with a new one, or the
    one built last time and its version (--skip-build)."""
    target_dir = os.path.join(ROOT, "target", "installer")
    exe = os.path.join(target_dir, TARGET, "release", "foc_esc.exe")
    noted = os.path.join(target_dir, "version.txt")
    if skip:
        if not os.path.isfile(exe):
            fail(f"--skip-build: there's no {exe} yet")
        if not os.path.isfile(noted):
            fail("--skip-build: the last foc_esc.exe was built without its version: build it again "
                 "(without --skip-build)")
        with open(noted) as f:
            return exe, f.read().strip()
    ver = version()
    step(f"Building foc_esc.exe {ver} (optimized, with the C runtime inside; the first time takes a while)")
    env = dict(os.environ, CARGO_TARGET_DIR=target_dir, RUSTFLAGS="-C target-feature=+crt-static",
               FOC_ESC_VERSION=ver)
    r = subprocess.run(["cargo", "build", "--release", "--bin", "foc_esc", "--target", TARGET],
                       cwd=ROOT, env=env)
    if r.returncode != 0:
        fail("the build failed (see above)")
    with open(noted, "w") as f:
        f.write(ver + "\n")
    return exe, ver


def check_exe(exe):
    needs = pe_imports(exe)
    crt = [d for d in needs if CRT_DLLS.match(d)]
    if crt:
        fail(f"foc_esc.exe still needs the C runtime's DLLs ({', '.join(crt)}): it would need "
             "Microsoft's Visual C++ Redistributable on the players' PCs")
    print(f"  foc_esc.exe: {os.path.getsize(exe) / 1e6:.0f} MB, needs only Windows' own DLLs "
          f"({len(needs)})")


def copy_tree(src, dst, skip_dir=lambda rel: False, skip_file=lambda rel: False):
    for dirpath, dirnames, filenames in os.walk(src):
        rel_dir = os.path.relpath(dirpath, src)
        rel_dir = "" if rel_dir == "." else rel_dir
        dirnames[:] = [d for d in dirnames
                       if d != "__pycache__" and not skip_dir(os.path.join(rel_dir, d))]
        os.makedirs(os.path.join(dst, rel_dir), exist_ok=True)
        for f in filenames:
            rel = os.path.join(rel_dir, f)
            if skip_file(rel):
                continue
            shutil.copy2(os.path.join(src, rel), os.path.join(dst, rel))


def stage_python(dst):
    base = sys.base_prefix
    if "windowsapps" in base.lower():
        fail("this Python is the Microsoft Store's, which can't be copied: install Python from "
             "https://www.python.org and run this with it")
    step(f"Copying this PC's Python {sys.version.split()[0]} ({base})")
    copy_tree(base, dst,
              skip_dir=lambda rel: os.path.normcase(rel.replace("\\", "/")) in PYTHON_SKIP_DIRS,
              skip_file=lambda rel: os.path.basename(rel) in PYTHON_SKIP_FILES or rel.endswith(".pdb"))
    for exe in ["python.exe", "pythonw.exe"]:
        if not os.path.isfile(os.path.join(dst, exe)):
            fail(f"no {exe} in {base}")
    site = os.path.join(dst, "Lib", "site-packages")
    os.makedirs(site, exist_ok=True)
    wanted = []
    for name in PACKAGES:
        try:
            wanted.append(f"{name}=={importlib.metadata.version(name)}")
        except importlib.metadata.PackageNotFoundError:
            wanted.append(name)
    step("Installing " + ", ".join(wanted) + " into it")
    r = subprocess.run([sys.executable, "-m", "pip", "install", "--disable-pip-version-check",
                        "--no-warn-script-location", "--only-binary=:all:", "--target", site] + wanted)
    if r.returncode != 0:
        fail("pip couldn't install the packages (see above)")


def find_umodel(given):
    """UE Viewer's umodel_64.exe (or umodel.exe) as extract_game.py finds it: the
    one given, or next to tools, in tools\\umodel, in the demo's folder or its
    umodel folder, or on the PATH."""
    names = ["umodel_64.exe", "umodel.exe"]
    if given:
        return os.path.abspath(given) if os.path.isfile(given) else None
    for folder in [HERE, os.path.join(HERE, "umodel"), ROOT, os.path.join(ROOT, "umodel")]:
        for n in names:
            if os.path.isfile(os.path.join(folder, n)):
                return os.path.join(folder, n)
    for n in names:
        found = shutil.which(n)
        if found:
            return found
    return None


def umodel_file(name):
    """One of UE Viewer's own files, to go with it: its programs, its SDL2 DLLs, its
    licence and readme (not exports or settings left in its folder)."""
    n = name.lower()
    return ((n.startswith("umodel") and n.endswith(".exe")) or (n.startswith("sdl2") and n.endswith(".dll"))
            or n in ("license.txt", "readme.txt"))


def stage_tools(dst, umodel_exe):
    step("Copying the asset build's tools and UE Viewer")
    found = find_umodel(umodel_exe)
    if not found:
        fail("UE Viewer wasn't found (umodel_64.exe or umodel.exe, with its SDL2 files): put it in "
             "tools\\umodel, or on the PATH as for extract assets.bat, or give it with --umodel")
    umodel = os.path.dirname(found)
    print(f"   UE Viewer: {found}")

    def skip_dir(rel):
        parts = rel.replace("\\", "/").split("/")
        return parts[0] in TOOLS_SKIP or (parts[0] == "installer" and parts[-1] in INSTALLER_SKIP)

    def skip_file(rel):
        parts = rel.replace("\\", "/").split("/")
        if parts[0] == "installer":
            return parts[-1] in INSTALLER_SKIP
        return rel in TOOLS_SKIP_FILES or rel.endswith((".log", ".pyc"))

    copy_tree(HERE, dst, skip_dir, skip_file)
    # (UE Viewer's own files, into the installed copy's tools\\umodel, where its
    # extract_game.py finds them.)
    copy_tree(umodel, os.path.join(dst, "umodel"), skip_dir=lambda rel: True,
              skip_file=lambda rel: not umodel_file(os.path.basename(rel)))


def pipeline_id(tools):
    """The build tools' id: a hash of the build's scripts and UE Viewer (all of
    tools\\ but the installer's own files)."""
    h = hashlib.sha256()
    files = []
    for dirpath, dirnames, filenames in os.walk(tools):
        rel_dir = os.path.relpath(dirpath, tools).replace("\\", "/")
        if rel_dir.split("/")[0] in ("installer", "ffmpeg"):
            continue
        for f in filenames:
            files.append((rel_dir + "/" + f).lstrip("./"))
    for rel in sorted(files):
        h.update(rel.encode() + b"\0")
        with open(os.path.join(tools, rel), "rb") as f:
            h.update(hashlib.sha256(f.read()).digest())
    return h.hexdigest()[:16]


def stage_rest(stage, exe):
    step("Copying foc_esc.exe, the demo's shaders, its icon and the licences")
    shutil.copy2(exe, os.path.join(stage, "foc_esc.exe"))
    sys.path.insert(0, HERE)
    import extract_game
    built = extract_game.BUILT_SHADERS
    own = os.path.join(ROOT, "assets", "shaders")
    names = sorted(n for n in os.listdir(own) if n.endswith(".wgsl") and n not in built)
    if not names:
        fail("no shaders in assets\\shaders")
    os.makedirs(os.path.join(stage, "assets", "shaders"))
    for n in names:
        shutil.copy2(os.path.join(own, n), os.path.join(stage, "assets", "shaders", n))
    os.makedirs(os.path.join(stage, "icon"))
    shutil.copy2(os.path.join(ROOT, "icon", "foc_esc.ico"), os.path.join(stage, "icon", "foc_esc.ico"))
    copy_tree(os.path.join(HERE, "installer", "licenses"), os.path.join(stage, "licenses"))
    print(f"  {len(names)} shaders")


def check_stage(stage):
    step("Checking the bundled Python runs the build's scripts and the builder's window")
    python = os.path.join(stage, "python", "python.exe")
    env = {k: v for k, v in os.environ.items() if k not in ("PYTHONHOME", "PYTHONPATH")}
    for cmd in [[python, "-E", "-s", "-c", "import numpy, PIL, fontTools, tkinter; print('packages ok')"],
                [python, "-E", "-s", os.path.join(stage, "tools", "extract_game.py"), "--list"],
                [python, "-E", "-s", os.path.join(stage, "tools", "installer", "builder.py"), "--self-test"]]:
        r = subprocess.run(cmd, cwd=os.path.join(stage, "tools"), env=env, capture_output=True, text=True)
        out = (r.stdout + r.stderr).strip()
        if r.returncode != 0:
            fail("the bundled Python failed: " + " ".join(cmd[3:]) + "\n" + out)
        print("  " + out.splitlines()[-1])
    # (Python's caches written by the checks aren't installed.)
    for dirpath, dirnames, _ in os.walk(stage):
        for d in list(dirnames):
            if d == "__pycache__":
                shutil.rmtree(os.path.join(dirpath, d))
                dirnames.remove(d)


def version():
    """The installer's version: today's date and the time, as Windows takes a
    version (four numbers)."""
    now = datetime.datetime.now()
    return f"{now.year}.{now.month}.{now.day}.{now.hour * 100 + now.minute}"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--iscc", help="Inno Setup's ISCC.exe")
    ap.add_argument("--umodel", help="UE Viewer's umodel_64.exe (otherwise found as extract_game.py finds it)")
    ap.add_argument("--skip-build", action="store_true", help="use the foc_esc.exe built last time")
    args = ap.parse_args()
    if os.name != "nt":
        fail("make the installer on Windows (it builds a Windows program and a Windows Setup)")
    iscc = find_iscc(args.iscc)
    if not iscc:
        fail("Inno Setup 6 isn't installed: get it from https://jrsoftware.org/isdl.php "
             "(or run: winget install JRSoftware.InnoSetup), then run this again")
    exe, ver = build_exe(args.skip_build)
    check_exe(exe)
    if os.path.exists(STAGE):
        shutil.rmtree(STAGE)
    os.makedirs(STAGE)
    stage_rest(STAGE, exe)
    stage_tools(os.path.join(STAGE, "tools"), args.umodel)
    stage_python(os.path.join(STAGE, "python"))
    pid = pipeline_id(os.path.join(STAGE, "tools"))
    with open(os.path.join(STAGE, "tools", "installer", "pipeline.txt"), "w") as f:
        f.write(pid + "\n")
    print(f"\n  build tools' id: {pid}")
    check_stage(STAGE)
    step(f"Compiling Setup {ver} with Inno Setup")
    os.makedirs(OUT, exist_ok=True)
    r = subprocess.run([iscc, "/Q", f"/DStage={STAGE}", f"/DAppVersion={ver}", f"/DOutputDir={OUT}", ISS])
    if r.returncode != 0:
        fail("Inno Setup couldn't compile it (see above)")
    setup = os.path.join(OUT, f"Cybertron Escalation Setup {ver}.exe")
    with open(setup, "rb") as f:
        sha = hashlib.sha256(f.read()).hexdigest()
    print(f"\nMade {setup}\n  {os.path.getsize(setup) / 1e6:.0f} MB, SHA-256 {sha}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
