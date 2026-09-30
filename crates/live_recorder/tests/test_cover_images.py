"""Offline regression tests for original-quality live-room covers."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "src" / "streamlink_worker.py"
spec = importlib.util.spec_from_file_location("cover_worker", SOURCE)
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


class Response:
    status_code = 200
    headers = {}
    encoding = "utf-8"
    url = "https://example.test/room"

    def __init__(self, body):
        self.body = body
        self.closed = False

    def iter_content(self, chunk_size):
        yield self.body

    def close(self):
        self.closed = True


class Session:
    def __init__(self, response):
        self.http = self
        self.response = response

    def get(self, *args, **kwargs):
        return self.response


class CoverTests(unittest.TestCase):
    def test_largest_declared_cover_wins_over_first_thumbnail(self):
        response = Response(b'''<meta property="og:image" content="/tiny.jpg">
            <meta property="og:image:width" content="64">
            <meta property="og:image:height" content="64">
            <meta property="og:image" content="/original.jpg?sig=keep&amp;size=large">
            <meta property="og:image:width" content="1280">
            <meta property="og:image:height" content="720">''')
        self.assertEqual(worker.page_cover_url(Session(response), response.url),
                         "https://example.test/original.jpg?sig=keep&size=large")
        self.assertTrue(response.closed)

    def test_unknown_sizes_preserve_publisher_fallback_order(self):
        response = Response(b'''<meta property="og:image" content="javascript:bad">
            <meta property="og:image" content="/cover.jpg">
            <meta property="og:image:width" content="broken">
            <meta name="twitter:image" content="/fallback.jpg">''')
        self.assertEqual(worker.page_cover_url(Session(response), response.url),
                         "https://example.test/cover.jpg")

    def test_cache_keeps_original_bytes_without_resizing(self):
        original = b"\x89PNG\r\n\x1a\n" + bytes(range(256)) * 20
        response = Response(original)
        with tempfile.TemporaryDirectory() as folder:
            cached = Path(folder) / "room.img"
            with patch.object(worker, "page_cover_url", return_value="https://example.test/original.png"):
                result = worker.cache_page_cover(Session(response), response.url, str(cached))
            self.assertEqual(result, str(cached))
            self.assertEqual(cached.read_bytes(), original)
            self.assertEqual(list(Path(folder).glob("*.tmp")), [])
        self.assertTrue(response.closed)

    def test_invalid_replacement_keeps_previous_cover(self):
        with tempfile.TemporaryDirectory() as folder:
            cached = Path(folder) / "room.img"
            previous = b"\xff\xd8\xffprevious-original"
            cached.write_bytes(previous)
            response = Response(b"<html>try again</html>")
            with patch.object(worker, "page_cover_url", return_value="https://example.test/new.jpg"):
                result = worker.cache_page_cover(Session(response), response.url, str(cached))
            self.assertEqual(result, str(cached))
            self.assertEqual(cached.read_bytes(), previous)
            self.assertEqual(list(Path(folder).glob("*.tmp")), [])


if __name__ == "__main__":
    unittest.main()
