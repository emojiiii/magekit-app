"""Private line-delimited JSON adapter. Credentials arrive on stdin, never argv.

probe: JSON metadata; record: JSON progress; pull: raw media (internal subprocess).
Each reconnect gets a fresh Streamlink session and a TS normalizer. A persistent final
muxer keeps a single output file; no raw FLV/fMP4 concatenation or file overwrites.
"""
import json
import logging
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import threading
import time
from urllib.parse import urlparse

OFFLINE, UNSUPPORTED, AUTH, FAILED = 10, 11, 12, 13
logging.disable(logging.CRITICAL)


def emit(**event):
    print(json.dumps(event, ensure_ascii=True), flush=True)


def matching_domain(host, domain):
    return bool(domain) and (host == domain or host.endswith("." + domain))


def cookie_scope(host, key):
    # Explicit aliases, NOT URL substring matching (soop KR and Global must not mix).
    aliases = {
        "douyin": "douyin.com", "bilibili": "bilibili.com", "huya": "huya.com",
        "douyu": "douyu.com", "kuaishou": "kuaishou.com", "twitch": "twitch.tv",
        "sooplive": "sooplive.co.kr", "soop": "sooplive.co.kr",
        "soop_global": "sooplive.com", "afreeca": "afreecatv.com",
    }
    key = key.strip().lower()
    domain = aliases.get(key)
    if domain is None:
        parsed = urlparse(key if "://" in key else "https://" + key)
        domain = parsed.hostname or ""
        if "." not in domain or parsed.path not in ("", "/"):
            return None
    return domain if matching_domain(host, domain) else None


def cookie_pairs(value):
    for part in value.split(";"):
        name, separator, contents = part.strip().partition("=")
        if separator and re.fullmatch(r"[!#$%&'*+.^_`|~0-9A-Za-z-]+", name):
            if "\r" not in contents and "\n" not in contents:
                yield name, contents


def make_session(request):
    from streamlink import Streamlink
    url = request["url"]
    parsed = urlparse(url)
    if parsed.scheme not in ("https", "http") or not parsed.hostname or parsed.username:
        raise ValueError("Only HTTP(S) room URLs without embedded credentials are accepted")
    config = request.get("config", {})
    timeout = max(1, min(30, int(config.get("timeout", 30))))
    session = Streamlink({
        "http-timeout": timeout, "stream-timeout": min(timeout, 15),
        "stream-segment-timeout": min(timeout, 15), "stream-segment-attempts": 3,
        "stream-segment-threads": 2, "no-plugin-cache": True,
        "http-trust-env": False, "webbrowser": False,
    })
    if request.get("ffmpeg"):
        session.set_option("ffmpeg-ffmpeg", request["ffmpeg"])
    if config.get("proxy"):
        session.set_option("http-proxy", config["proxy"])
    for key, value in config.get("headers", {}).items():
        if "\r" in key + value or "\n" in key + value:
            raise ValueError("HTTP headers must not contain newlines")
        if key.lower() == "cookie":
            for name, contents in cookie_pairs(value):
                session.http.cookies.set(name, contents, domain=parsed.hostname, path="/",
                                         secure=parsed.scheme == "https")
        elif key.lower() in ("authorization", "proxy-authorization", "host"):
            raise ValueError("Use scoped cookies or plugin authentication, not global auth/Host headers")
        else:
            session.http.headers[key] = value
    # Domain entries take precedence over named aliases; only one cookie configuration is applied.
    candidates = []
    for cookie in request.get("cookies", []):
        if not cookie.get("enabled", False):
            continue
        domain = cookie_scope(parsed.hostname, cookie.get("platform", ""))
        if domain:
            candidates.append(("." in cookie["platform"], domain, cookie["cookie"]))
    if candidates:
        _, domain, value = sorted(candidates, key=lambda row: row[0], reverse=True)[0]
        for name, contents in cookie_pairs(value):
            session.http.cookies.set(name, contents, domain=domain, path="/",
                                     secure=parsed.scheme == "https")
    # 直接复用探测阶段的 SOOP HLS 地址时，不会再经过插件的请求头注入逻辑。
    # 补上插件原本使用的 Referer/Origin，避免 CDN 对录制请求返回空流。
    if any(matching_domain(parsed.hostname, domain) for domain in
           ("sooplive.com", "sooplive.co.kr", "afreecatv.com")):
        session.http.headers["Referer"] = url
        session.http.headers["Origin"] = "https://play.sooplive.com"
    return session


def resolve(request):
    session = make_session(request)
    name, cls, url = session.resolve_url(request["url"], follow_redirect=False)
    plugin = cls(session, url)
    return session, name, plugin


def choose_stream(streams, quality):
    if not streams:
        return None
    if quality in ("Original", "Blue"):
        return streams.get("best", next(iter(streams.values())))
    if quality == "Low":
        return streams.get("worst", next(iter(streams.values())))
    target = {"Ultra": 1080, "High": 720, "Standard": 480}.get(quality, 1080)
    named = [(int(match.group(1)), name) for name in streams
             if (match := re.match(r"^(\d+)p", name))]
    if not named:
        return streams.get("best", next(iter(streams.values())))
    _, selected = min(named, key=lambda pair: (abs(pair[0] - target), -pair[0], pair[1]))
    return streams[selected]


def quality_name(name, stream, streams):
    if stream is streams.get("best"):
        return "Original"
    match = re.match(r"^(\d+)p", name)
    height = int(match.group(1)) if match else 1080
    return "Ultra" if height >= 1080 else "High" if height >= 720 else "Standard" if height >= 480 else "Low"


def probe(request):
    session, name, plugin = resolve(request)
    try:
        streams = plugin.streams()
        parsed = urlparse(request["url"])
        room_id = str(plugin.id or parsed.path.rstrip("/").rsplit("/", 1)[-1] or parsed.hostname)[:256]
        room = dict(room_id=room_id, anchor_name=str(plugin.author or room_id)[:256],
                    title=str(plugin.title or room_id)[:512], status="Live" if streams else "Offline",
                    start_time=None, viewer_count=None, cover_url=None,
                    extra={"streamlink_plugin": name, "streamlink_qualities": list(streams)[:100]})
        data = []
        for quality, stream in list(streams.items())[:100]:
            if quality in ("best", "worst"):
                continue
            urls = dict(hls_url=None, flv_url=None, dash_url=None)
            try:
                url = stream.to_url()
                if isinstance(url, str) and url.startswith(("http://", "https://")):
                    kind = stream.shortname()
                    if kind == "hls": urls["hls_url"] = url
                    elif kind == "dash": urls["dash_url"] = url
                    elif ".flv" in urlparse(url).path.lower(): urls["flv_url"] = url
            except (AttributeError, NotImplementedError, TypeError):
                pass  # Some Streamlink streams are opaque/muxed, not standalone HTTP URLs.
            data.append(dict(quality=quality_name(quality, stream, streams), url=urls,
                             bitrate=None, resolution=None, codec=None, cdn=quality))
        emit(kind="probe", room=room, streams=data)
    finally:
        session.http.close()


def classify(error):
    from streamlink.exceptions import NoPluginError
    if isinstance(error, NoPluginError):
        return UNSUPPORTED, "No Streamlink plugin matches this room URL"
    # Never return raw plugin errors: they can contain signed URLs or cookie values.
    text = str(error).lower()
    if any(word in text for word in ("authentication", "unauthorized", "login", "log in", "age verification", "403", "401")):
        return AUTH, "Stream requires a valid login/access permission; check the room's scoped cookies"
    return FAILED, "Streamlink failed to resolve/read the stream; check network, room URL and plugin version"


def pull(request):
    if os.name != "nt":
        signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    session = make_session(request)
    source = None
    try:
        direct_url = request.get("stream_url")
        if direct_url:
            from streamlink.stream.hls import HLSStream
            parsed = urlparse(direct_url)
            if parsed.scheme not in ("http", "https"):
                raise ValueError("Streamlink returned an invalid HLS URL")
            source_stream = HLSStream(session, direct_url)
        else:
            name, cls, resolved_url = session.resolve_url(request["url"], follow_redirect=False)
            del name
            plugin = cls(session, resolved_url)
            source_stream = choose_stream(
                plugin.streams(), request.get("config", {}).get("quality", "Original")
            )
        if source_stream is None:
            return OFFLINE
        source = source_stream.open()
        while True:
            chunk = source.read(64 * 1024)
            if not chunk:
                return 0
            sys.stdout.buffer.write(chunk)
            sys.stdout.buffer.flush()
    finally:
        if source is not None:
            source.close()
        session.http.close()


def command(arguments, **kwargs):
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NO_WINDOW
    return subprocess.Popen(arguments, **kwargs)


def terminate(process):
    if process is None or process.poll() is not None:
        return
    if os.name == "nt":
        # Also clean up FFmpeg processes created internally by a Streamlink muxed stream.
        try:
            command(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).wait(timeout=5)
        except (OSError, subprocess.TimeoutExpired):
            process.kill()
    else:
        process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=3)


def worker_source():
    if "-c" in sys.orig_argv:
        return sys.orig_argv[sys.orig_argv.index("-c") + 1]
    return Path(__file__).read_text(encoding="utf-8")


def mux_args(ffmpeg, output, format_name):
    formats = {"ts": "mpegts", "mp4": "mp4", "mkv": "matroska", "flv": "flv"}
    if format_name not in formats:
        raise ValueError("Supported recording formats: ts, mp4, mkv, flv")
    args = [ffmpeg, "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
            "-fflags", "+genpts+discardcorrupt", "-i", "pipe:0", "-map", "0:v?", "-map", "0:a?",
            "-c", "copy", "-avoid_negative_ts", "make_zero"]
    if format_name == "mp4":
        # Do not use empty_moov: it bypasses automatic ADTS-to-ASC handling for copied AAC.
        args += ["-movflags", "+frag_keyframe+default_base_moof"]
    return args + ["-f", formats[format_name], str(output)]


def record_direct_ts(request):
    """直接保存 Streamlink 已输出的 MPEG-TS，避免直播管道等待 EOF 才 flush。"""
    config = request["config"]
    output = Path(request["output"])
    stop = threading.Event()

    def control():
        # EOF also means the Rust owner went away. Never leave a detached recording running.
        for line in sys.stdin:
            if line.strip() == "stop":
                break
        stop.set()

    threading.Thread(target=control, daemon=True).start()
    started = time.monotonic()
    last_size = 0
    last_report = started
    total_input = [0]
    source = None
    terminal, message = "completed", None
    maximum = config.get("max_duration")
    timeout = max(20, int(config.get("timeout", 30)))
    retries = max(0, min(100, int(config.get("retry_count", 3))))

    def progress(state):
        nonlocal last_size, last_report
        now = time.monotonic()
        size = output.stat().st_size if output.exists() else 0
        speed = max(0, int((size - last_size) / max(0.01, now - last_report)))
        last_size, last_report = size, now
        emit(kind="progress", status=state, duration=int(now - started), size=size, speed=speed)

    try:
        with output.open("wb") as target:
            for attempt in range(retries + 1):
                if stop.is_set():
                    terminal = "stopped"
                    break
                if maximum is not None and time.monotonic() - started >= maximum:
                    break

                progress("connecting")
                request_pull = dict(request, mode="pull")
                source = command(
                    [sys.executable, "-I", "-u", "-c", worker_source()],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
                source_process = source
                source.stdin.write((json.dumps(request_pull) + "\n").encode())
                source.stdin.close()
                done = threading.Event()
                pump_errors = []
                last_data = [time.monotonic()]

                def pump():
                    try:
                        while chunk := source_process.stdout.read(64 * 1024):
                            target.write(chunk)
                            target.flush()
                            total_input[0] += len(chunk)
                            last_data[0] = time.monotonic()
                    except (OSError, ValueError) as error:
                        pump_errors.append(type(error).__name__)
                    finally:
                        done.set()

                pump_thread = threading.Thread(target=pump, daemon=True)
                pump_thread.start()
                interrupted = False
                while not done.wait(0.25):
                    now = time.monotonic()
                    if now - last_report >= 1:
                        progress("recording" if total_input[0] else "connecting")
                    if stop.is_set() or (maximum is not None and now - started >= maximum):
                        terminal = "stopped" if stop.is_set() else "completed"
                        interrupted = True
                        break
                    if now - last_data[0] >= timeout:
                        break

                code = source.poll()
                if done.is_set() and code is None:
                    try:
                        code = source.wait(timeout=1)
                    except subprocess.TimeoutExpired:
                        pass
                source_stderr = source.stderr
                terminate(source)
                source_message = ""
                if source_stderr is not None:
                    try:
                        source_message = source_stderr.read().decode("utf-8", errors="replace").strip()
                    except (OSError, ValueError):
                        source_message = ""
                if not done.wait(5):
                    raise RuntimeError("Media stream pump failed to stop")
                pump_thread.join(timeout=1)
                source = None

                if interrupted:
                    break
                if pump_errors:
                    raise RuntimeError("Media stream pump failed; existing output is preserved")
                if code in (AUTH, UNSUPPORTED):
                    raise RuntimeError(source_message or "Stream access/plugin error; check login permissions and plugin version")
                if total_input[0] > 0 and code in (0, OFFLINE):
                    break
                if attempt == retries:
                    raise RuntimeError(source_message or "Stream ended/stalled and the reconnect budget was exhausted")

                progress("connecting")
                until = time.monotonic() + min(30, 2 ** attempt)
                while time.monotonic() < until and not stop.wait(0.25):
                    if maximum is not None and time.monotonic() - started >= maximum:
                        break

        if total_input[0] == 0 and terminal != "stopped":
            raise RuntimeError("No media was recorded")
    except Exception as error:
        terminal = "error"
        message = str(error) if isinstance(error, RuntimeError) else "Recording pipeline failed; check output permissions"
    finally:
        terminate(source)
    progress(terminal)
    emit(kind="finished", status=terminal, message=message,
         duration=int(time.monotonic() - started), size=last_size, speed=0)
    return 0 if terminal != "error" else FAILED


def record(request):
    config = request["config"]
    if config.get("segment_duration") is not None or config.get("include_danmaku"):
        raise ValueError("Streamlink backend does not yet implement timed file splitting or danmaku")
    if config.get("format") == "ts" and request.get("stream_url"):
        return record_direct_ts(request)
    output = Path(request["output"])
    ffmpeg = request["ffmpeg"]
    stop = threading.Event()
    def control():
        # EOF also means the Rust owner went away. Never leave a detached recording running.
        for line in sys.stdin:
            if line.strip() == "stop":
                break
        stop.set()
    threading.Thread(target=control, daemon=True).start()
    started = time.monotonic()
    last_size = 0
    last_report = started
    total_input = [0]
    muxer = source = normalizer = None
    terminal, message = "completed", None
    maximum = config.get("max_duration")
    timeout = max(20, int(config.get("timeout", 30)))
    retries = max(0, min(100, int(config.get("retry_count", 3))))
    done = threading.Event()
    pump_errors = []
    pump_thread = None

    def progress(state):
        nonlocal last_size, last_report
        now = time.monotonic()
        size = output.stat().st_size if output.exists() else 0
        speed = max(0, int((size - last_size) / max(0.01, now - last_report)))
        last_size, last_report = size, now
        emit(kind="progress", status=state, duration=int(now - started), size=size, speed=speed)

    try:
        muxer = command(mux_args(ffmpeg, output, config["format"]), stdin=subprocess.PIPE,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for attempt in range(retries + 1):
            if stop.is_set():
                terminal = "stopped"
                break
            if maximum is not None and time.monotonic() - started >= maximum:
                break
            progress("connecting")
            request_pull = dict(request, mode="pull")
            source = command([sys.executable, "-I", "-u", "-c", worker_source()],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            source.stdin.write((json.dumps(request_pull) + "\n").encode())
            source.stdin.close()
            # Normalize each connection before reconnecting to the persistent final muxer.
            normalizer = command([ffmpeg, "-hide_banner", "-loglevel", "error", "-nostdin",
                                  "-fflags", "+genpts+discardcorrupt", "-i", "pipe:0",
                                  "-map", "0:v?", "-map", "0:a?", "-c", "copy",
                                  "-avoid_negative_ts", "make_zero", "-mpegts_flags",
                                  "+resend_headers+initial_discontinuity", "-f", "mpegts", "pipe:1"],
                                 stdin=source.stdout, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            source.stdout.close()
            done.clear()
            pump_errors.clear()
            pipe = normalizer.stdout
            def pump():
                try:
                    while chunk := pipe.read(64 * 1024):
                        muxer.stdin.write(chunk)
                        muxer.stdin.flush()
                        total_input[0] += len(chunk)
                except (OSError, ValueError) as error:
                    pump_errors.append(type(error).__name__)
                finally:
                    done.set()
            pump_thread = threading.Thread(target=pump, daemon=True)
            pump_thread.start()
            sample, last_data = total_input[0], time.monotonic()
            interrupted = False
            while not done.wait(0.25):
                now = time.monotonic()
                if total_input[0] != sample:
                    sample, last_data = total_input[0], now
                if now - last_report >= 1:
                    progress("recording" if sample else "connecting")
                if stop.is_set() or (maximum is not None and now - started >= maximum):
                    terminal = "stopped" if stop.is_set() else "completed"
                    interrupted = True
                    break
                if now - last_data >= timeout or muxer.poll() is not None:
                    break  # bounded stall recovery, even when a process remains alive
            code = source.poll()
            source_message = ""
            if done.is_set() and code is None:
                try:
                    code = source.wait(timeout=1)
                except subprocess.TimeoutExpired:
                    pass
            source_stderr = source.stderr
            terminate(source)
            if source_stderr is not None:
                try:
                    source_message = source_stderr.read().decode("utf-8", errors="replace").strip()
                except (OSError, ValueError):
                    source_message = ""
            # EOF from the source lets the normalizer and pump drain before another attempt.
            if not done.wait(5):
                terminate(normalizer)
            if not done.wait(3):
                raise RuntimeError("Media pump failed to stop")
            pump_thread.join(timeout=1)
            terminate(normalizer)
            pipe.close()
            source = normalizer = None
            if interrupted:
                break
            if pump_errors or muxer.poll() is not None:
                raise RuntimeError("FFmpeg pipeline failed; existing output is preserved")
            if code == OFFLINE and total_input[0] > 0:
                break
            if code in (AUTH, UNSUPPORTED):
                raise RuntimeError(source_message or "Stream access/plugin error; check login permissions and plugin version")
            if attempt == retries:
                raise RuntimeError(source_message or "Stream ended/stalled and the reconnect budget was exhausted")
            # An EOF is not a byte-range resume. Resolve a fresh plugin/session on the next attempt.
            progress("connecting")
            until = time.monotonic() + min(30, 2 ** attempt)
            while time.monotonic() < until and not stop.wait(0.25):
                if maximum is not None and time.monotonic() - started >= maximum:
                    break
                if time.monotonic() - last_report >= 1:
                    progress("connecting")
        muxer.stdin.close()
        try:
            code = muxer.wait(timeout=10)
        except subprocess.TimeoutExpired:
            raise RuntimeError("FFmpeg could not finalize the output in time")
        if code != 0 and total_input[0] > 0:
            raise RuntimeError("FFmpeg finalization failed; existing output is preserved")
        if total_input[0] == 0 and terminal != "stopped":
            raise RuntimeError("No media was recorded")
    except Exception as error:
        terminal = "error"
        # Only errors authored by this adapter are exposed. No raw URLs/headers from subprocesses.
        message = str(error) if isinstance(error, RuntimeError) else "Recording pipeline failed; check FFmpeg and output permissions"
    finally:
        terminate(source)
        terminate(normalizer)
        if muxer is not None:
            terminate(muxer)
        if pump_thread is not None:
            pump_thread.join(timeout=1)
    progress(terminal)
    emit(kind="finished", status=terminal, message=message,
         duration=int(time.monotonic() - started), size=last_size, speed=0)
    return 0 if terminal != "error" else FAILED


def main():
    request = json.loads(sys.stdin.readline())
    mode = request.get("mode", "probe")
    try:
        if mode == "record":
            return record(request)
        if mode == "pull":
            return pull(request)
        probe(request)
        return 0
    except Exception as error:
        code, message = classify(error)
        if mode == "pull":
            # 父进程只读取媒体 stdout；错误摘要走 stderr，避免污染 TS 数据。
            print(message, file=sys.stderr, flush=True)
        else:
            emit(kind="unsupported" if code == UNSUPPORTED else "error", message=message)
        return code


if __name__ == "__main__":
    sys.exit(main())
