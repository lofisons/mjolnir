#!/usr/bin/env python3
"""Align an APK: every entry stored without compression starts on a 4-byte
boundary, as Android asks of resources.arsc and of anything it maps straight
from the package. Entries are copied byte for byte; only padding is added.

Usage:  python3 zipalign.py in.apk out.apk
"""

import struct
import sys


def align(src, dst):
    data = open(src, "rb").read()
    end = data.rfind(b"PK\x05\x06")
    count = struct.unpack("<H", data[end + 10 : end + 12])[0]
    cd_size, cd_off = struct.unpack("<II", data[end + 12 : end + 20])
    out = bytearray()
    central = bytearray()
    pos = cd_off
    for _ in range(count):
        assert data[pos : pos + 4] == b"PK\x01\x02", "bad central directory"
        head = bytearray(data[pos : pos + 46])
        flags, method = struct.unpack("<HH", head[8:12])
        crc, csize, usize = struct.unpack("<III", head[16:28])
        nlen, elen, clen = struct.unpack("<HHH", head[28:34])
        local = struct.unpack("<I", head[42:46])[0]
        name = data[pos + 46 : pos + 46 + nlen]
        rest = data[pos + 46 + nlen : pos + 46 + nlen + elen + clen]
        pos += 46 + nlen + elen + clen

        lhead = bytearray(data[local : local + 30])
        lnlen, lelen = struct.unpack("<HH", lhead[26:30])
        start = local + 30 + lnlen + lelen
        body = data[start : start + csize]
        # sizes go in the header itself, not after the data
        flags &= ~0x08
        lhead[6:8] = struct.pack("<H", flags)
        lhead[14:26] = struct.pack("<III", crc, csize, usize)
        offset = len(out)
        pad = (-(offset + 30 + nlen)) % 4 if method == 0 else 0
        lhead[28:30] = struct.pack("<H", pad)
        out += lhead + name + b"\0" * pad + body

        head[8:10] = struct.pack("<H", flags)
        head[42:46] = struct.pack("<I", offset)
        central += head + name + rest
    cd_start = len(out)
    out += central
    tail = bytearray(data[end : end + 22])
    tail[12:20] = struct.pack("<II", len(central), cd_start)
    tail[20:22] = b"\0\0"
    out += tail
    open(dst, "wb").write(out)


if __name__ == "__main__":
    align(sys.argv[1], sys.argv[2])
