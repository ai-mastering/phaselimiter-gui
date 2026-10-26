#!/usr/bin/env python3
"""Build pinned backend dependencies and the backend, without modifying its source."""
import argparse
import json
import os
import platform
import shutil
import subprocess

from audio_common import Build, PINS, download, extract_deb, extract_tar, run


def dependencies(build):
    prefix = build.prefix
    build.cmake("zlib", build.source("zlib"))
    zlib = prefix / "lib" / ("zlibstatic.lib" if build.is_windows else "libz.a")
    build.cmake("sndfile", build.source("sndfile"), [
        "-DBUILD_SHARED_LIBS=OFF", "-DENABLE_EXTERNAL_LIBS=OFF", "-DENABLE_MPEG=OFF",
        "-DBUILD_PROGRAMS=OFF", "-DBUILD_EXAMPLES=OFF", "-DENABLE_CPACK=OFF"])
    # libpng 1.6.43 has a host/target mixup in its pkg-config installation rule.
    # Native builds on each runner avoid the cross-install workaround from the prototype.
    build.cmake("png", build.source("png"), [
        "-DPNG_SHARED=OFF", "-DPNG_STATIC=ON", "-DPNG_TESTS=OFF", "-DPNG_TOOLS=OFF",
        f"-DZLIB_LIBRARY={zlib}", f"-DZLIB_INCLUDE_DIR={prefix / 'include'}"])
    build.cmake("jpeg", build.source("jpeg"), [
        "-DENABLE_SHARED=OFF", "-DENABLE_STATIC=ON", "-DWITH_SIMD=OFF", "-DWITH_TURBOJPEG=OFF"])
    build.cmake("tbb", build.source("tbb"), [
        "-DTBB_TEST=OFF", "-DTBB_EXAMPLES=OFF", "-DTBB_STRICT=OFF", "-DTBB_ENABLE_IPO=OFF",
        "-DTBB_DISABLE_HWLOC_AUTOMATIC_SEARCH=ON",
        "-DCMAKE_INSTALL_RPATH=" + ("@loader_path" if build.is_mac else "$ORIGIN")])
    omp = build.source("openmp")
    shutil.copytree(build.source("llvm-cmake"), omp.parent / "cmake", dirs_exist_ok=True)
    build.cmake("openmp", omp, [
        "-DOPENMP_ENABLE_LIBOMPTARGET=OFF", "-DOPENMP_ENABLE_OMPT_TOOLS=OFF",
        "-DOPENMP_MSVC_NAME_SCHEME=OFF", "-DLIBOMP_OMPT_SUPPORT=OFF", "-DLIBOMP_USE_HWLOC=OFF",
        "-DLIBOMP_USE_ITT_NOTIFY=OFF", "-DLIBOMP_ENABLE_SHARED=ON"])
    boost = build.source("boost")
    libraries = "filesystem,serialization,system,math,iostreams"
    if build.is_windows:
        run(["cmd", "/c", "bootstrap.bat"], cwd=boost)
        b2 = boost / "b2.exe"
        toolset = "msvc"
    else:
        toolset = "clang" if build.is_mac else "gcc"
        run(["bash", "bootstrap.sh", f"--with-toolset={toolset}", f"--with-libraries={libraries}"], cwd=boost)
        b2 = boost / "b2"
    options = [f"toolset={toolset}", "variant=release", "link=static", "threading=multi", "cxxstd=14"]
    if build.is_windows:
        options += ["runtime-link=static", "address-model=64"]
    if build.is_mac:
        options += ["target-os=darwin", "address-model=64",
                    "architecture=arm" if platform.machine() == "arm64" else "architecture=x86",
                    "cxxflags=-mmacosx-version-min=14.0", "cflags=-mmacosx-version-min=14.0"]
    run([b2, *options, "--layout=system", f"--build-dir={build.work / 'build/boost'}",
         f"--prefix={prefix}", *[f"--with-{name}" for name in libraries.split(",")],
         "-sNO_ZLIB=1", "-sNO_BZIP2=1", "-sNO_LZMA=1", "-sNO_ZSTD=1",
         f"-j{build.jobs}", "install"], cwd=boost)


def ipp_options(build):
    if build.is_mac and platform.machine() == "arm64":
        return []  # backend selects Apple's system Accelerate framework
    key = "windows" if build.is_windows else "macos-intel" if build.is_mac else "linux"
    manifest = json.loads((build.backend / "environment/dependencies.json").read_text())
    sdk = build.work / "sdk"
    for asset in manifest[key]:
        if "ipp" not in asset["name"]:
            continue
        path = download(asset, build.downloads)
        destination = sdk / asset["destination"]
        marker = sdk / (asset["sha256"] + ".extracted")
        if not marker.exists():
            if asset["format"] == "deb":
                extract_deb(path, destination)
            else:
                extract_tar(path, destination)
            marker.touch()
    header = next(sdk.rglob("ipp.h")).parent
    options = [f"-DIPP_INCLUDE_DIR={header}"]
    for name in ["ippi", "ipps", "ippvm", "ippcore"]:
        filename = name + "mt.lib" if build.is_windows else "lib" + name + ".a"
        matches = sorted(path for path in sdk.rglob(filename) if "nonpic" not in path.parts)
        if len(matches) != 1:
            raise ValueError(f"Expected one static IPP library {filename}, found {matches}")
        options.append(f"-DIPP_{name}_LIBRARY={matches[0]}")
    return options


def backend(build):
    snapshot = build.backend / "SOURCE-REVISION"
    actual = snapshot.read_text().strip() if snapshot.is_file() else subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=build.backend, text=True).strip()
    if actual != PINS["backend"]["revision"]:
        raise ValueError(f"Wrong backend revision: {actual}")
    prefix = build.prefix
    # Only headers are used; do not link the Debian Armadillo binaries on other OSes.
    armadillo = build.source("armadillo") / "include"
    if not (armadillo / "armadillo").exists():
        armadillo = build.source("armadillo") / "usr/include"
    zlib = prefix / "lib" / ("zlibstatic.lib" if build.is_windows else "libz.a")
    png = prefix / "lib" / ("libpng16_static.lib" if build.is_windows else "libpng16.a")
    jpeg = prefix / "lib" / ("jpeg-static.lib" if build.is_windows else "libjpeg.a")
    omp = prefix / "lib" / ("libomp.lib" if build.is_windows else "libomp.dylib" if build.is_mac else "libomp.so")
    cxx_flags = ["-DARMA_DONT_USE_SUPERLU"]
    if build.is_mac and platform.machine() == "arm64":
        # macOS 14 supports the current Accelerate CBLAS interface. Keep its
        # default 32-bit integer ABI to match the backend's existing int sizes.
        cxx_flags.append("-DACCELERATE_NEW_LAPACK")
    options = ["-DBUILD_TESTING=ON", "-DSANDYBRIDGE_SUPPORT=ON", "-DCMAKE_CXX_STANDARD=14",
               # The Debian header package enables SuperLU with an absolute
               # /usr/include path. optim does not use sparse linear solvers;
               # disable that optional integration on every target platform.
               # Append to the toolchain defaults, preserving MSVC's /EHsc.
               "-DCMAKE_CXX_FLAGS_INIT=" + " ".join(cxx_flags),
               "-DPHASELIMITER_STATIC_BOOST=ON", f"-DCMAKE_PREFIX_PATH={prefix}",
               f"-DARMADILLO_INCLUDE_DIR={armadillo}", f"-DZLIB_LIBRARY={zlib}",
               f"-DZLIB_LIBRARY_RELEASE={zlib}", f"-DPNG_LIBRARY_RELEASE={png}",
               f"-DJPEG_LIBRARY={jpeg}", "-DOpenMP_CXX_LIB_NAMES=omp",
               f"-DOpenMP_omp_LIBRARY={omp}", f"-DOpenMP_CXX_INCLUDE_DIR={prefix / 'include'}",
               *ipp_options(build)]
    if build.is_windows:
        options += ["-DBoost_USE_STATIC_RUNTIME=ON", "-DOpenMP_CXX_FLAGS=/openmp:llvm"]
    elif build.is_mac:
        options += ["-DOpenMP_CXX_FLAGS=-Xpreprocessor -fopenmp", "-DCMAKE_INSTALL_RPATH=@loader_path/../lib"]
    else:
        options += [f"-DOpenMP_CXX_FLAGS=-fopenmp -I{prefix / 'include'}"]
    # BUILD_TESTING=ON overrides the generic dependency build default.
    build.cmake("backend", build.backend, options)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=["source", "dependencies", "backend"])
    parser.add_argument("name", nargs="?")
    parser.add_argument("--work", default="build/audio")
    parser.add_argument("--jobs", default=os.environ.get("BUILD_JOBS", "2"))
    args = parser.parse_args()
    build = Build(args.work, args.jobs)
    if args.phase == "source":
        print(build.source(args.name))
    elif args.phase == "dependencies":
        dependencies(build)
    else:
        backend(build)


if __name__ == "__main__":
    main()
