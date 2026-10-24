#!/usr/bin/env python3
"""Inspect an x64 PE executable's normal and delay-load DLL imports."""
import json
import pathlib
import struct
import sys

path = pathlib.Path(sys.argv[1])
data = path.read_bytes()
pe = struct.unpack_from("<I", data, 0x3C)[0]
if data[pe:pe + 4] != b"PE\0\0":
    raise SystemExit("Not a PE executable")
machine, sections = struct.unpack_from("<HH", data, pe + 4)
optional_size = struct.unpack_from("<H", data, pe + 20)[0]
optional = pe + 24
if machine != 0x8664 or struct.unpack_from("<H", data, optional)[0] != 0x20B:
    raise SystemExit("Expected an x86-64 PE32+ executable")
image_base = struct.unpack_from("<Q", data, optional + 24)[0]
section_table = optional + optional_size


def offset(rva):
    for index in range(sections):
        position = section_table + index * 40
        virtual_size, address, raw_size, raw_pointer = struct.unpack_from("<IIII", data, position + 8)
        if address <= rva < address + max(virtual_size, raw_size):
            return raw_pointer + rva - address
    raise ValueError(f"RVA not mapped: {rva:#x}")


def string(rva):
    start = offset(rva)
    return data[start:data.index(b"\0", start)].decode("ascii")


def imports(directory, delayed=False):
    rva, size = struct.unpack_from("<II", data, optional + 112 + directory * 8)
    if not rva:
        return []
    position = offset(rva)
    result = []
    width = 32 if delayed else 20
    while any(data[position:position + width]):
        values = struct.unpack_from("<8I" if delayed else "<5I", data, position)
        name = values[1] if delayed else values[3]
        if delayed and not values[0] & 1:
            name -= image_base
        result.append(string(name))
        position += width
    return sorted(set(result), key=str.lower)


normal = imports(1)
delayed = imports(13, delayed=True)
runtime = [name for name in normal + delayed if name.lower().startswith(
    ("vcruntime", "msvcp", "api-ms-win-crt-", "ucrtbase", "msvcr"))]
report = {
    "architecture": "x86_64",
    "subsystem": struct.unpack_from("<H", data, optional + 68)[0],
    "imports": normal,
    "delay_imports": delayed,
    "vc_runtime_dll_imports": runtime,
    "scope": "PE import tables only; runtime LoadLibrary calls and other processes are not traced",
}
print(json.dumps(report, indent=2))
if "--check-no-vc-runtime" in sys.argv[2:] and runtime:
    raise SystemExit("Unexpected VC runtime DLL imports: " + ", ".join(runtime))
