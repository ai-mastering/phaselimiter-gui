"""Collect runtime dependencies and relocate them inside a portable audio bundle."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

from audio_common import ROOT, run

TOOLS = ("phase_limiter", "audio_analyzer", "audio_detector", "snd_file_info",
         "spectrogram", "audio_visualizer", "clear_mixer", "effect_test")
GLIBC = re.compile(r"^(lib(c|m|dl|pthread|rt|resolv|util|anl)\.so\.|ld-linux|linux-vdso)")
WINDOWS_SYSTEM = set("""advapi32 bcrypt bcryptprimitives cfgmgr32 combase comctl32 comdlg32
    crypt32 dbghelp dnsapi dwmapi gdi32 imm32 kernel32 kernelbase msvcrt msimg32 ntdll normaliz
    ole32 oleaut32 powrprof psapi rpcrt4 sechost secur32 setupapi shell32 shlwapi user32 userenv
    uxtheme version win32u winhttp winmm winspool ws2_32 wtsapi32""".split())


def pe(path):
    return json.loads(subprocess.check_output(
        [os.sys.executable, str(ROOT / "script/analyze-windows.py"), str(path)], text=True))


def linux_dependencies(binary):
    result = subprocess.check_output(["ldd", str(binary)], text=True)
    if "not found" in result:
        raise RuntimeError(f"Unresolved dependency: {binary}\n{result}")
    return [Path(match) for match in re.findall(r"=> (/\S+) \(", result)
            if not GLIBC.match(Path(match).name)]


def mac_dependencies(binary):
    output = subprocess.check_output(["otool", "-L", str(binary)], text=True)
    libraries = [line.strip().split(" (", 1)[0] for line in output.splitlines()[1:]]
    return libraries[1:] if binary.suffix == ".dylib" else libraries


def collect(stage, prefix):
    """The stage contains backend/bin, backend/lib and ffmpeg; inspect all shipped tools."""
    lib = stage / "phaselimiter/lib"
    lib.mkdir(parents=True, exist_ok=True)
    bindir = stage / "phaselimiter/bin"
    windows = os.name == "nt"
    mac = not windows and os.uname().sysname == "Darwin"
    pending = [*bindir.iterdir(), stage / ("ffmpeg.exe" if windows else "ffmpeg")]
    seen = set()
    report = {}
    while pending:
        binary = pending.pop()
        if binary in seen:
            continue
        seen.add(binary)
        if windows:
            imports = pe(binary)
            report[str(binary.relative_to(stage))] = imports
            if imports["vc_runtime_dll_imports"] and binary.name != "ffmpeg.exe":
                raise RuntimeError(f"Expected static MSVC CRT: {binary}: {imports['vc_runtime_dll_imports']}")
            for name in imports["imports"] + imports["delay_imports"]:
                if name.lower().startswith(("api-ms-", "ext-ms-")) or name.lower().removesuffix(".dll") in WINDOWS_SYSTEM:
                    continue
                destination = bindir / name
                if not destination.exists():
                    found = [path for path in (prefix / "bin").glob("*.dll") if path.name.lower() == name.lower()]
                    if len(found) != 1:
                        raise RuntimeError(f"Missing bundled DLL {name} required by {binary}")
                    shutil.copy2(found[0], destination)
                pending.append(destination)
        elif mac:
            dependencies = mac_dependencies(binary)
            report[str(binary.relative_to(stage))] = dependencies
            for name in dependencies:
                if name.startswith(("/usr/lib/", "/System/Library/")):
                    continue
                destination = lib / Path(name).name
                if not destination.exists():
                    matches = sorted((prefix / "lib").glob(Path(name).name))
                    if len(matches) != 1:
                        raise RuntimeError(f"Missing dylib {name} required by {binary}")
                    shutil.copy2(matches[0], destination)
                run(["install_name_tool", "-change", name, "@rpath/" + destination.name, binary])
                pending.append(destination)
            if binary.parent == lib:
                run(["install_name_tool", "-id", "@rpath/" + binary.name, binary])
            relative = os.path.relpath(lib, binary.parent)
            rpath = "@loader_path" if relative == "." else "@loader_path/" + relative
            commands = subprocess.check_output(["otool", "-l", str(binary)], text=True)
            existing = re.findall(r"cmd LC_RPATH\s+cmdsize \d+\s+path (.*?) \(offset", commands)
            for old in existing:
                if old != rpath:
                    run(["install_name_tool", "-delete_rpath", old, binary])
            if rpath not in existing:
                run(["install_name_tool", "-add_rpath", rpath, binary])
            # Relocation invalidates the linker signature. Ad-hoc signing needs no certificate.
            run(["codesign", "--force", "--sign", "-", binary])
        else:
            dependencies = linux_dependencies(binary)
            report[str(binary.relative_to(stage))] = [path.name for path in dependencies]
            for source in dependencies:
                destination = lib / source.name
                if not destination.exists():
                    shutil.copy2(source, destination)
                pending.append(destination)
            relative = os.path.relpath(lib, binary.parent)
            rpath = "$ORIGIN" if relative == "." else "$ORIGIN/" + relative
            run(["patchelf", "--set-rpath", rpath, binary])
    (stage / "runtime-dependencies.json").write_text(json.dumps(report, indent=2) + "\n")


def audit(stage):
    """Reject unresolved or development-prefix dependencies after relocation."""
    windows = os.name == "nt"
    mac = not windows and os.uname().sysname == "Darwin"
    # ZIP omits empty directories; Windows keeps its DLLs in bin, not lib.
    binaries = [*(stage / "phaselimiter/bin").iterdir(), *(stage / "phaselimiter/lib").glob("*"),
                stage / ("ffmpeg.exe" if windows else "ffmpeg")]
    for binary in binaries:
        if windows:
            info = pe(binary)
            for name in info["imports"] + info["delay_imports"]:
                if name.lower().startswith(("api-ms-", "ext-ms-")) or name.lower().removesuffix(".dll") in WINDOWS_SYSTEM:
                    continue
                if not any(p.name.lower() == name.lower() for p in (stage / "phaselimiter/bin").iterdir()):
                    raise RuntimeError(f"Unresolved DLL: {binary}: {name}")
        elif mac:
            for name in mac_dependencies(binary):
                if name.startswith(("/usr/lib/", "/System/Library/")):
                    continue
                if not name.startswith("@rpath/") or not (stage / "phaselimiter/lib" / Path(name).name).is_file():
                    raise RuntimeError(f"Unresolved dylib: {binary}: {name}")
        else:
            for path in linux_dependencies(binary):
                if not path.resolve().is_relative_to(stage.resolve()):
                    raise RuntimeError(f"Dependency outside bundle: {binary}: {path}")
