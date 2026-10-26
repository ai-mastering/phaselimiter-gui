"""Shared paths, verified source downloads, and native build helpers."""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
PINS = json.loads((ROOT / "packaging/audio-sources.json").read_text())


def run(command, **kwargs):
    print("+", subprocess.list2cmdline([str(arg) for arg in command]), flush=True)
    return subprocess.run([str(arg) for arg in command], check=True, **kwargs)


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def download(asset, cache):
    cache.mkdir(parents=True, exist_ok=True)
    path = cache / asset.get("filename", asset["url"].rsplit("/", 1)[-1])
    if not path.exists():
        temporary = path.with_name(path.name + ".part")
        try:
            with urllib.request.urlopen(asset["url"], timeout=120) as response:
                with temporary.open("wb") as output:
                    shutil.copyfileobj(response, output)
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)
    if digest(path) != asset["sha256"]:
        raise ValueError(f"SHA-256 mismatch; remove and retry: {path}")
    return path


def extract_tar(source, destination, include_prefixes=()):
    destination.mkdir(parents=True, exist_ok=True)
    base = destination.resolve()
    with tarfile.open(fileobj=source) if hasattr(source, "read") else tarfile.open(source) as archive:
        for item in archive.getmembers():
            path = base / item.name
            if not path.resolve().is_relative_to(base) or item.isdev() or item.isfifo():
                raise ValueError(f"Unsafe archive member: {item.name}")
            if item.issym() or item.islnk():
                target = (path.parent if item.issym() else base) / item.linkname
                if not target.resolve().is_relative_to(base):
                    raise ValueError(f"Unsafe archive link: {item.name}")
        members = [item for item in archive.getmembers()
                   if not include_prefixes or item.name.removeprefix("./").startswith(include_prefixes)]
        # Also supports the Python 3.11 devcontainer (before tarfile.data_filter).
        if hasattr(tarfile, "data_filter"):
            archive.extractall(base, members=members, filter="data")
        else:
            archive.extractall(base, members=members)


def extract_deb(path, destination, include_prefixes=()):
    """Read Debian ar/data.tar archives without requiring dpkg on Windows/macOS."""
    with path.open("rb") as package:
        if package.read(8) != b"!<arch>\n":
            raise ValueError(f"Not a Debian ar archive: {path}")
        while header := package.read(60):
            name = header[:16].decode("ascii").strip().rstrip("/")
            size = int(header[48:58])
            data = package.read(size)
            if size % 2:
                package.read(1)
            if name.startswith("data.tar"):
                extract_tar(io.BytesIO(data), destination, include_prefixes)
                return
    raise ValueError(f"No data archive: {path}")


class Build:
    def __init__(self, work, jobs):
        self.work = Path(work).resolve()
        self.jobs = jobs
        self.downloads = self.work / "downloads"
        self.sources = self.work / "sources"
        self.prefix = self.work / "prefix"
        self.backend = self.work / "backend"
        self.is_windows = os.name == "nt"
        self.is_mac = os.uname().sysname == "Darwin" if not self.is_windows else False

    def source(self, name):
        asset = PINS["sources"][name]
        archive = download(asset, self.downloads)
        destination = self.sources / name
        marker = destination / ".extracted"
        try:
            extracted = json.loads(marker.read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            extracted = {}
        if not isinstance(extracted, dict) or extracted.get("sha256") != asset["sha256"]:
            shutil.rmtree(destination, ignore_errors=True)
            extract = destination / "unpacked"
            if archive.suffix == ".deb":
                # Armadillo is header-only here. Its Linux library symlink has
                # no target in this dev package and cannot be extracted on
                # Windows without symlink privileges. Do not install it.
                include = ("usr/include/", "usr/share/doc/libarmadillo-dev/copyright") if name == "armadillo" else ()
                extract_deb(archive, extract, include)
            else:
                extract_tar(archive, extract)
            children = list(extract.iterdir())
            root = children[0] if len(children) == 1 and children[0].is_dir() else extract
            extracted = {"sha256": asset["sha256"], "root": str(root.relative_to(destination))}
            marker.write_text(json.dumps(extracted))
        return destination / extracted["root"]

    def cmake(self, name, source, options=()):
        directory = self.work / "build" / name
        flags = ["-G", "Ninja", "-DCMAKE_BUILD_TYPE=Release",
                 f"-DCMAKE_INSTALL_PREFIX={self.prefix}", "-DCMAKE_INSTALL_LIBDIR=lib",
                 "-DCMAKE_POLICY_DEFAULT_CMP0091=NEW", "-DBUILD_TESTING=OFF"]
        if self.is_windows:
            flags += ["-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded"]
        if self.is_mac:
            flags += ["-DCMAKE_C_COMPILER=/usr/bin/clang", "-DCMAKE_CXX_COMPILER=/usr/bin/clang++",
                      "-DCMAKE_OSX_DEPLOYMENT_TARGET=14.0"]
        run(["cmake", "-S", source, "-B", directory, *flags, *options])
        run(["cmake", "--build", directory, "--parallel", self.jobs])
        run(["cmake", "--install", directory])
        return directory
