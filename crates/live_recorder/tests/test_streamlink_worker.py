"""Offline contract tests, plus real FFmpeg remux/cancellation tests when installed."""
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import time
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src" / "streamlink_worker.py"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


worker = load("magekit_streamlink_worker", SOURCE)
bundler = load("magekit_uv_bundler", ROOT / "build_support" / "bundle_uv.py")


class ContractTests(unittest.TestCase):
    def test_cookie_domain_boundary(self):
        self.assertTrue(worker.matching_domain("play.sooplive.co.kr", "sooplive.co.kr"))
        self.assertFalse(worker.matching_domain("sooplive.co.kr.evil.test", "sooplive.co.kr"))
        self.assertFalse(worker.matching_domain("evilsooplive.co.kr", "sooplive.co.kr"))

    def test_soop_credentials_cannot_cross_regions(self):
        self.assertEqual(worker.cookie_scope("play.sooplive.co.kr", "sooplive"), "sooplive.co.kr")
        self.assertIsNone(worker.cookie_scope("play.sooplive.com", "sooplive"))
        self.assertIsNone(worker.cookie_scope("play.sooplive.co.kr", "sooplive.com"))
        self.assertEqual(worker.cookie_scope("play.sooplive.com", "sooplive.com"), "sooplive.com")

    def test_cookie_configuration_url_scope(self):
        self.assertEqual(worker.cookie_scope("live.bilibili.com", "https://bilibili.com/"), "bilibili.com")
        self.assertIsNone(worker.cookie_scope("live.bilibili.com", "https://bilibili.com/path"))

    def test_cookie_newlines_rejected_and_equals_preserved(self):
        self.assertEqual(list(worker.cookie_pairs("token=a=b==; broken; bad name=x; x=a\r\nb")), [("token", "a=b==")])

    def test_quality_selection_and_fallback(self):
        low, high, best = object(), object(), object()
        streams = {"360p": low, "720p": high, "1080p": best, "best": best, "worst": low}
        self.assertIs(worker.choose_stream(streams, "High"), high)
        self.assertIs(worker.choose_stream(streams, "Original"), best)
        self.assertIs(worker.choose_stream(streams, "Low"), low)
        self.assertIsNone(worker.choose_stream({}, "High"))
        self.assertIs(worker.choose_stream({"live": best, "best": best}, "High"), best)

    def test_mp4_is_a_muxer_not_a_file_extension(self):
        args = worker.mux_args("ffmpeg", "/tmp/output.mp4", "mp4")
        self.assertEqual(args[-3:], ["-f", "mp4", "/tmp/output.mp4"])
        self.assertIn("+frag_keyframe+default_base_moof", args)
        self.assertIn("copy", args)
        with self.assertRaises(ValueError):
            worker.mux_args("ffmpeg", "x.m3u8", "m3u8")

    def test_worker_fits_windows_command_line(self):
        args = [r"C:\Program Files\MageKit\python.exe", "-I", "-u", "-c", SOURCE.read_text()]
        self.assertLess(len(subprocess.list2cmdline(args).encode("utf-16-le")) // 2, 30000)

    def test_target_mapping_is_not_host_architecture(self):
        self.assertEqual(bundler.TARGETS["aarch64-apple-darwin"], "aarch64-apple-darwin")
        self.assertEqual(bundler.TARGETS["x86_64-unknown-linux-gnu"], "x86_64-unknown-linux-musl")

    def test_archive_extracts_only_executable_without_traversal(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            info = tarfile.TarInfo("../../uv")
            info.size = 6
            archive.addfile(info, io.BytesIO(b"binary"))
        self.assertEqual(bundler.binary_from_archive(buffer.getvalue(), False), b"binary")
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            archive.writestr("dir/uv.exe", b"windows")
        self.assertEqual(bundler.binary_from_archive(buffer.getvalue(), True), b"windows")

    def test_archive_rejects_ambiguous_executables(self):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            archive.writestr("first/uv.exe", b"a")
            archive.writestr("second/uv.exe", b"b")
        with self.assertRaises(ValueError):
            bundler.binary_from_archive(buffer.getvalue(), True)


@unittest.skipUnless(shutil.which("ffmpeg") and shutil.which("ffprobe"), "FFmpeg/ffprobe not installed")
class MediaPipelineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.fixture = self.root / "fixture.ts"
        subprocess.run([shutil.which("ffmpeg"), "-hide_banner", "-loglevel", "error", "-f", "lavfi",
                        "-i", "testsrc2=size=160x90:rate=15", "-f", "lavfi", "-i", "sine=frequency=440",
                        "-t", "3", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-f", "mpegts",
                        str(self.fixture)], check=True, timeout=15)
        package = self.root / "streamlink"
        package.mkdir()
        (package / "exceptions.py").write_text("class NoPluginError(Exception): pass\n")
        (package / "__init__.py").write_text(textwrap.dedent('''
            import io, time
            from pathlib import Path
            from types import SimpleNamespace
            ROOT = Path(__file__).parent.parent
            class Jar:
                def set(self, *a, **kw): pass
            class Stream:
                def open(self):
                    data = (ROOT / "fixture.ts").read_bytes()
                    if (ROOT / "continuous").exists():
                        class Loop:
                            def read(self, size):
                                time.sleep(0.05)
                                return data
                            def close(self): pass
                        return Loop()
                    return io.BytesIO(data)
            class Plugin:
                id, author, title = "room", "author", "title"
                def __init__(self, *a): pass
                def streams(self):
                    counter = ROOT / "counter"
                    number = int(counter.read_text()) + 1 if counter.exists() else 1
                    counter.write_text(str(number))
                    if (ROOT / "error").exists():
                        raise RuntimeError("network error with SECRET_TOKEN=do-not-leak")
                    sessions = int((ROOT / "sessions").read_text()) if (ROOT / "sessions").exists() else 1
                    if number > sessions and not (ROOT / "continuous").exists():
                        return {}
                    stream = Stream()
                    return {"720p": stream, "best": stream, "worst": stream}
            class Streamlink:
                def __init__(self, *a):
                    self.http = SimpleNamespace(cookies=Jar(), headers={}, close=lambda: None)
                def set_option(self, *a): pass
                def resolve_url(self, url, **kw): return "test", Plugin, url
        '''), encoding="utf-8")

    def tearDown(self):
        self.temp.cleanup()

    def start_worker(self, **config):
        source = "import sys; sys.path.insert(0, " + repr(str(self.root)) + ")\n" + SOURCE.read_text()
        request = {"mode": "record", "url": "https://test.invalid/room", "cookies": [],
                   "ffmpeg": shutil.which("ffmpeg"), "output": str(self.root / "output.mp4"),
                   "config": {"format": "mp4", "quality": "Original", "headers": {},
                              "retry_count": 2, "timeout": 20, **config}}
        process = subprocess.Popen([sys.executable, "-I", "-u", "-c", source], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        process.stdin.write(json.dumps(request) + "\n")
        process.stdin.flush()
        self.addCleanup(lambda: process.poll() is None and process.kill())
        return process

    def finish(self, process):
        # Keep stdin open: EOF is intentionally a stop command in the worker protocol.
        process.wait(timeout=35)
        output, errors = process.stdout.read(), process.stderr.read()
        process.stdin.close()
        process.stdout.close()
        process.stderr.close()
        events = [json.loads(line) for line in output.splitlines()]
        self.assertTrue(events, errors)
        return events, output + errors

    def test_reconnect_then_offline_finalizes_real_mp4(self):
        process = self.start_worker()
        events, _ = self.finish(process)
        self.assertEqual(events[-1]["status"], "completed", events)
        self.assertEqual((self.root / "counter").read_text(), "2")
        self.assertGreater(events[-1]["size"], 0)
        result = subprocess.run([shutil.which("ffprobe"), "-v", "error", "-show_entries", "format=format_name",
                                 "-of", "json", str(self.root / "output.mp4")], capture_output=True, text=True, check=True)
        self.assertIn("mp4", json.loads(result.stdout)["format"]["format_name"])

    def test_two_media_connections_preserve_duration(self):
        (self.root / "sessions").write_text("2")
        process = self.start_worker(retry_count=3)
        events, _ = self.finish(process)
        self.assertEqual(events[-1]["status"], "completed", events)
        self.assertEqual((self.root / "counter").read_text(), "3")
        result = subprocess.run([shutil.which("ffprobe"), "-v", "error", "-count_frames",
                                 "-show_entries", "stream=nb_read_frames:format=duration", "-of", "json",
                                 str(self.root / "output.mp4")], capture_output=True, text=True, check=True)
        details = json.loads(result.stdout)
        self.assertGreater(float(details["format"]["duration"]), 5.0, details)
        self.assertGreaterEqual(int(details["streams"][0]["nb_read_frames"]), 80, details)

    def test_stop_acknowledges_after_file_is_finalized(self):
        (self.root / "continuous").touch()
        process = self.start_worker()
        time.sleep(2)
        process.stdin.write("stop\n")
        process.stdin.flush()
        events, _ = self.finish(process)
        self.assertEqual(events[-1]["status"], "stopped", events)
        self.assertGreater(events[-1]["size"], 0)
        subprocess.run([shutil.which("ffprobe"), "-v", "error", str(self.root / "output.mp4")], check=True, timeout=5)

    def test_max_duration_completes_without_a_stop_request(self):
        (self.root / "continuous").touch()
        process = self.start_worker(max_duration=2)
        events, _ = self.finish(process)
        self.assertEqual(events[-1]["status"], "completed", events)

    def test_retry_budget_and_secret_redaction(self):
        (self.root / "error").touch()
        process = self.start_worker(retry_count=1)
        events, output = self.finish(process)
        self.assertEqual(events[-1]["status"], "error")
        self.assertEqual((self.root / "counter").read_text(), "2")
        self.assertNotIn("SECRET_TOKEN", output)
        self.assertNotIn("do-not-leak", output)


if __name__ == "__main__":
    unittest.main()
