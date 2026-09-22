"""Build-host helper: verify a pinned official uv archive and embed only its executable.

No third-party Python modules. Never runs downloaded code and never extracts paths from
an archive onto the filesystem. Python is needed on the build host, NOT the end-user PC.
"""
import hashlib
import io
import os
from pathlib import Path
import re
import sys
import tarfile
import time
import urllib.request
import uuid
import zipfile

VERSION = "0.12.17"
TARGETS = {
    "x86_64-pc-windows-msvc": "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc": "aarch64-pc-windows-msvc",
    "x86_64-apple-darwin": "x86_64-apple-darwin",
    "aarch64-apple-darwin": "aarch64-apple-darwin",
    "x86_64-unknown-linux-gnu": "x86_64-unknown-linux-musl",
    "x86_64-unknown-linux-musl": "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-gnu": "aarch64-unknown-linux-musl",
    "aarch64-unknown-linux-musl": "aarch64-unknown-linux-musl",
}


def fetch(url, limit):
    request = urllib.request.Request(url, headers={"User-Agent": "MageKit-uv-bundler"})
    for attempt in range(3):
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                data = response.read(limit + 1)
            if len(data) > limit:
                raise ValueError("uv release asset exceeds allowed size")
            return data
        except (OSError, TimeoutError):
            if attempt == 2:
                raise
            time.sleep(2 ** attempt)


def binary_from_archive(data, windows):
    expected = "uv.exe" if windows else "uv"
    if windows:
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            entries = [item for item in archive.infolist()
                       if not item.is_dir() and Path(item.filename).name == expected]
            if len(entries) != 1 or entries[0].file_size > 128 * 1024 * 1024:
                raise ValueError("unexpected uv archive contents")
            return archive.read(entries[0])
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        entries = [item for item in archive.getmembers()
                   if item.isfile() and Path(item.name).name == expected]
        if len(entries) != 1 or entries[0].size > 128 * 1024 * 1024:
            raise ValueError("unexpected uv archive contents")
        with archive.extractfile(entries[0]) as stream:
            return stream.read()


def main(target, destination):
    if target not in TARGETS:
        raise ValueError("unsupported target; supply an explicit MAGEKIT_UV_BUNDLE")
    windows = "windows" in target
    name = "uv-" + TARGETS[target] + (".zip" if windows else ".tar.gz")
    base = "https://github.com/astral-sh/uv/releases/download/" + VERSION + "/"
    checksum_text = fetch(base + name + ".sha256", 4096).decode("ascii")
    checksum = checksum_text.split()[0]
    if not re.fullmatch(r"[a-fA-F0-9]{64}", checksum):
        raise ValueError("invalid official checksum")
    data = fetch(base + name, 128 * 1024 * 1024)
    if hashlib.sha256(data).hexdigest() != checksum.lower():
        raise ValueError("uv archive SHA-256 mismatch")
    binary = binary_from_archive(data, windows)
    path = Path(destination)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + "." + uuid.uuid4().hex + ".tmp")
    try:
        temporary.write_bytes(binary)
        temporary.chmod(0o700)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)
    print("Embedded uv " + VERSION + " for " + target)


if __name__ == "__main__":
    main(*sys.argv[1:])
