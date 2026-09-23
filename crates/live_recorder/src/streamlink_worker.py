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
from html.parser import HTMLParser
from urllib.parse import urljoin, urlparse

OFFLINE, UNSUPPORTED, AUTH, FAILED, TIMEOUT = 10, 11, 12, 13, 14
logging.disable(logging.CRITICAL)


class SoopAuthError(Exception):
    pass


class SoopResolutionTimeout(Exception):
    pass


def emit(**event):
    print(json.dumps(event, ensure_ascii=True), flush=True)


def matching_domain(host, domain):
    return bool(domain) and (host == domain or host.endswith("." + domain))


def is_soop_host(host):
    return any(matching_domain(host, domain) for domain in (
        "sooplive.com", "sooplive.co.kr", "afreecatv.com",
    ))


def cookie_scope(host, key):
    # Explicit aliases, NOT URL substring matching. The SOOP plugin uses .com
    # auth/API endpoints even for .co.kr rooms, so regional cookies must be
    # available to those first-party endpoints as well.
    aliases = {
        "douyin": "douyin.com", "bilibili": "bilibili.com", "huya": "huya.com",
        "douyu": "douyu.com", "kuaishou": "kuaishou.com", "twitch": "twitch.tv",
        "sooplive": "sooplive.co.kr", "soop": "sooplive.co.kr",
        "soop_global": "sooplive.com", "afreeca": "afreecatv.com",
    }
    key = key.strip().lower()
    if key in ("sooplive", "soop") and is_soop_host(host):
        return "sooplive.co.kr" if matching_domain(host, "sooplive.co.kr") else "sooplive.com"
    domain = aliases.get(key)
    # Keep the explicit Global alias bound to the plugin's .com auth/API domain.
    if key == "soop_global" and any(
        matching_domain(host, region)
        for region in ("sooplive.com", "sooplive.co.kr", "afreecatv.com")
    ):
        return "sooplive.com"
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
    proxy = config.get("proxy")
    trust_env = proxy == "system"
    session = Streamlink({
        "http-timeout": timeout, "stream-timeout": min(timeout, 15),
        "stream-segment-timeout": min(timeout, 15), "stream-segment-attempts": 3,
        "stream-segment-threads": 2, "no-plugin-cache": True,
        "http-trust-env": trust_env, "webbrowser": False,
    })
    if request.get("ffmpeg"):
        session.set_option("ffmpeg-ffmpeg", request["ffmpeg"])
    if proxy and not trust_env:
        session.set_option("http-proxy", proxy)
    for key, value in config.get("headers", {}).items():
        if "\r" in key + value or "\n" in key + value:
            raise ValueError("HTTP headers must not contain newlines")
        if key.lower() == "cookie":
            domains = [parsed.hostname]
            if matching_domain(parsed.hostname, "sooplive.co.kr"):
                domains.append(".sooplive.com")
            elif matching_domain(parsed.hostname, "sooplive.com"):
                domains = [".sooplive.com"]
            for name, contents in cookie_pairs(value):
                for domain in domains:
                    session.http.cookies.set(name, contents, domain=domain, path="/",
                                             secure=parsed.scheme == "https")
        elif key.lower() in ("authorization", "proxy-authorization", "host"):
            raise ValueError("Use scoped cookies or plugin authentication, not global auth/Host headers")
        else:
            session.http.headers[key] = value
    if is_soop_host(parsed.hostname):
        session.http.headers.setdefault("Referer", url)
        session.http.headers.setdefault("Origin", "https://play.sooplive.com")
    # Prefer explicit domain entries within a cookie domain. SOOP can need a
    # Korean room cookie and a Global auth cookie at the same time, so keep one
    # selected entry per destination domain instead of dropping one region.
    candidates = {}
    for cookie in request.get("cookies", []):
        if not cookie.get("enabled", False):
            continue
        domain = cookie_scope(parsed.hostname, cookie.get("platform", ""))
        if domain:
            platform = cookie.get("platform", "").strip().lower()
            priority = (platform == "soop_global", "." in platform)
            current = candidates.get(domain)
            if current is None or priority > current[0]:
                candidates[domain] = (priority, cookie["cookie"])
    # A Korean SOOP cookie belongs on .co.kr room pages, while the Streamlink
    # plugin performs login checks and channel lookups on .sooplive.com. Keep a
    # separately selected Global cookie authoritative; otherwise mirror the
    # explicitly selected Korean cookie to the plugin's first-party API domain.
    if is_soop_host(parsed.hostname) and "sooplive.com" not in candidates:
        korean_cookie = candidates.get("sooplive.co.kr") or candidates.get("afreecatv.com")
        if korean_cookie is not None:
            candidates["sooplive.com"] = korean_cookie
    if candidates:
        for domain, (_, value) in candidates.items():
            # Streamlink 8.6.1 的 SOOP 插件会精确查询 `.sooplive.com` 域 Cookie。
            cookie_domain = ".sooplive.com" if domain == "sooplive.com" else domain
            for name, contents in cookie_pairs(value):
                session.http.cookies.set(name, contents, domain=cookie_domain, path="/",
                                         secure=parsed.scheme == "https")
    return session


def resolve(request):
    session = make_session(request)
    name, cls, url = session.resolve_url(request["url"], follow_redirect=False)
    options = {}
    if name == "soop":
        credentials = request.get("soop_credentials") or {}
        username = credentials.get("username")
        password = credentials.get("password")
        if isinstance(username, str) and username.strip() and isinstance(password, str) and password:
            options = {"username": username, "password": password}
    plugin = cls(session, url, options=options)
    return session, name, plugin


def plugin_streams(name, plugin):
    if name != "soop":
        return plugin.streams()

    messages = []

    class Capture(logging.Handler):
        def emit(self, record):
            messages.append(record.getMessage().lower())

    logger = logging.getLogger(f"streamlink.plugins.{name}")
    handler = Capture()
    sink = logging.NullHandler()
    old_level = logger.level
    old_propagate = logger.propagate
    old_disable = logging.root.manager.disable
    root_sink = logging.NullHandler()
    logger.setLevel(logging.DEBUG)
    logger.propagate = False
    logger.addHandler(handler)
    logger.addHandler(sink)
    logging.getLogger().addHandler(root_sink)
    logging.disable(logging.NOTSET)
    try:
        streams = plugin.streams()
    finally:
        logging.disable(old_disable)
        logging.getLogger().removeHandler(root_sink)
        logger.removeHandler(handler)
        logger.removeHandler(sink)
        logger.setLevel(old_level)
        logger.propagate = old_propagate

    if not streams:
        combined = " ".join(messages)
        if "authentication using stored credentials has failed" in combined:
            raise SoopAuthError(
                "SOOP 已收到 Cookie，但登录校验失败；请确认 Cookie 未过期且账号已完成房间要求的认证"
            )
        login_errors = (
            "login required",
            "login has failed",
            "authentication using stored credentials has failed",
        )
        if any(error in combined for error in login_errors):
            raise SoopAuthError("SOOP 需要有效登录或观看权限；19+ 房间要求已完成成人认证的账号")
        if "stream is password protected" in combined:
            raise SoopAuthError("SOOP 直播间需要额外的观看权限或直播密码")
    return streams


def usable_soop_hint(plugin, hint):
    """Reuse recent room metadata from the status check; never reuse a stream key."""
    if not isinstance(hint, dict) or hint.get("channel") != plugin.match["channel"]:
        return None
    broadcast = plugin.match["bno"]
    if broadcast and str(hint.get("broadcast")) != str(broadcast):
        return None
    try:
        age = time.time() - float(hint.get("checked_at"))
    except (TypeError, ValueError):
        return None
    if not 0 <= age <= 90:
        return None
    if not all(isinstance(hint.get(key), str) and hint[key]
               for key in ("room_id", "rmd", "cdn")):
        return None
    if not hint["rmd"].startswith("https://"):
        return None
    presets = hint.get("presets")
    if not isinstance(presets, list) or not all(
        isinstance(item, dict) and isinstance(item.get("name"), str)
        and isinstance(item.get("label"), str) for item in presets
    ):
        return None
    return hint


def usable_prepared_soop_stream(hint, requested_quality, quality_offset):
    if not isinstance(hint, dict) or quality_offset or not isinstance(hint.get("prepared"), dict):
        return None
    prepared = hint["prepared"]
    if prepared.get("requested_quality") != str(requested_quality):
        return None
    try:
        age = time.time() - float(prepared.get("checked_at"))
    except (TypeError, ValueError):
        return None
    if not 0 <= age <= 90:
        return None
    if not all(isinstance(prepared.get(key), str) and prepared[key]
               for key in ("name", "label", "view_url", "aid")):
        return None
    if not prepared["view_url"].startswith(("https://", "http://")):
        return None
    return prepared


def soop_selected_stream(plugin, requested_quality, quality_offset=0, hint=None):
    """Resolve only the requested SOOP quality instead of asking every preset for a key."""
    from streamlink.exceptions import NoStreamsError
    from streamlink.plugins.soop import SoopHLSStream

    plugin.session.http.headers.update({
        "Referer": plugin.url,
        "Origin": "https://play.sooplive.com",
    })
    original_timeout = float(plugin.session.http.timeout or 30)
    deadline = time.monotonic() + 20

    def bound_api_timeout():
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise SoopResolutionTimeout()
        plugin.session.http.timeout = min(original_timeout, 8.0, remaining)

    try:
        channel = plugin.match["channel"]
        cached = usable_soop_hint(plugin, hint)
        prepared = usable_prepared_soop_stream(cached, requested_quality, quality_offset)
        if prepared is not None:
            plugin.magekit_selected_quality = {
                "name": prepared["name"][:64], "label": prepared["label"][:64],
                "cached_metadata": True, "prefetched": True,
            }
            return SoopHLSStream(
                plugin.session, prepared["view_url"], params={"aid": prepared["aid"]}
            )
        if cached is not None:
            room_id = cached["room_id"]
            author, title = cached.get("author"), cached.get("title")
            rmd, cdn = cached["rmd"], cached["cdn"]
            broadcast_password = cached.get("broadcast_password")
            presets = cached["presets"]
        else:
            try:
                bound_api_timeout()
                broadcast = plugin.match["bno"] or plugin._get_bno()
            except NoStreamsError:
                return None
            bound_api_timeout()
            result, room_id, author, title, rmd, cdn, broadcast_password, presets = plugin._get_channel_info(
                channel, broadcast
            )
            if result == plugin.CHANNEL_LOGIN_REQUIRED:
                username = plugin.get_option("username")
                password = plugin.get_option("password")
                bound_api_timeout()
                if not (username and password and plugin._login(username, password)):
                    cookie_present = bool(plugin.session.http.cookies.get_dict(domain=".sooplive.com"))
                    if cookie_present:
                        raise SoopAuthError(
                            "SOOP 已收到 Cookie，但频道仍要求登录或观看权限；请确认 Cookie 有效且账号已完成房间要求的认证"
                        )
                    raise SoopAuthError(
                        "SOOP 需要有效登录或观看权限；19+ 房间要求已完成成人认证的账号"
                    )
                bound_api_timeout()
                result, room_id, author, title, rmd, cdn, broadcast_password, presets = plugin._get_channel_info(
                    channel, broadcast
                )
            if result == plugin.CHANNEL_LOGIN_REQUIRED:
                raise SoopAuthError(
                    "SOOP 登录后仍无观看权限；19+ 房间需要已完成成人认证的账号"
                )
            if result != plugin.CHANNEL_RESULT_OK or not room_id or not rmd:
                return None

        plugin.id, plugin.author, plugin.title = room_id, author, title
        candidates = [item for item in presets if item.get("name") != "auto"]
        if not candidates:
            return None

        def height(item):
            for text in (item.get("label", ""), item.get("name", "")):
                match = re.search(r"(\d+)\s*p", str(text), re.IGNORECASE)
                if match:
                    return int(match.group(1))
            return {"original": 1080, "hd4k": 720, "hd": 540, "sd": 360}.get(
                item.get("name", "").lower()
            )

        quality = str(requested_quality)
        target = {"Ultra": 1080, "High": 720, "Standard": 480}.get(quality)
        indexed = list(enumerate(candidates))
        if quality == "Low":
            indexed.sort(key=lambda pair: (height(pair[1]) is None, height(pair[1]) or 0, pair[0]))
        elif target is not None:
            indexed.sort(key=lambda pair: (
                height(pair[1]) is None,
                abs((height(pair[1]) or target) - target),
                -(height(pair[1]) or 0),
                pair[0],
            ))
        else:
            indexed.sort(key=lambda pair: (
                height(pair[1]) is None,
                -(height(pair[1]) or 0),
                pair[0],
            ))

        # 原画由用户明确选定，不允许首包慢时静默降到 720p。
        offset = max(0, min(int(quality_offset), len(indexed) - 1))
        choices = indexed[offset:offset + (1 if quality in ("Original", "Blue") else 2)]
        for _, item in choices:
            bound_api_timeout()
            result, aid = plugin._get_hls_key(
                channel, room_id, item["name"], plugin.get_option("stream-password")
            )
            if result == plugin.CHANNEL_LOGIN_REQUIRED:
                username = plugin.get_option("username")
                password = plugin.get_option("password")
                if username and password:
                    bound_api_timeout()
                    if plugin._login(username, password):
                        bound_api_timeout()
                        result, aid = plugin._get_hls_key(
                            channel, room_id, item["name"], plugin.get_option("stream-password")
                        )
                if result == plugin.CHANNEL_LOGIN_REQUIRED:
                    raise SoopAuthError(
                        "SOOP 直播流授权要求登录或观看权限；请确认 Cookie 和账号认证状态"
                    )
            if result != plugin.CHANNEL_RESULT_OK:
                continue
            bound_api_timeout()
            view_url = plugin._get_stream_info(rmd, cdn, room_id, item["name"])
            if view_url:
                plugin.magekit_selected_quality = {
                    "name": item["name"][:64], "label": item["label"][:64],
                    "cached_metadata": cached is not None,
                }
                if isinstance(aid, str) and aid:
                    plugin.magekit_prepared = {
                        "checked_at": time.time(), "requested_quality": quality,
                        "name": item["name"], "label": item["label"],
                        "view_url": view_url, "aid": aid,
                    }
                return SoopHLSStream(plugin.session, view_url, params={"aid": aid})
        if broadcast_password == plugin.STREAM_PASSWORD_PROTECTED:
            raise SoopAuthError("SOOP 直播间需要额外的观看权限或直播密码")
        return None
    except SoopResolutionTimeout:
        raise
    except Exception as error:
        if "timeout" in type(error).__name__.lower():
            raise SoopResolutionTimeout() from None
        raise
    finally:
        plugin.session.http.timeout = original_timeout


def soop_is_live(plugin):
    """Check SOOP channel metadata without resolving every HLS quality variant."""
    from streamlink.exceptions import NoStreamsError

    plugin.session.http.headers.update({
        "Referer": plugin.url,
        "Origin": "https://play.sooplive.com",
    })

    username = plugin.get_option("username")
    password = plugin.get_option("password")
    cookie_present = bool(plugin.session.http.cookies.get_dict(domain=".sooplive.com"))
    login_attempted = False
    if not cookie_present and username and password:
        login_attempted = True
        plugin._login(username, password)

    channel = plugin.match["channel"]
    try:
        broadcast = plugin.match["bno"] or plugin._get_bno()
    except NoStreamsError:
        return False

    result, room_id, author, title, rmd, cdn, broadcast_password, presets = plugin._get_channel_info(
        channel, broadcast
    )
    if result == plugin.CHANNEL_LOGIN_REQUIRED and username and password and not login_attempted:
        if plugin._login(username, password):
            result, room_id, author, title, rmd, cdn, broadcast_password, presets = plugin._get_channel_info(
                channel, broadcast
            )
    if result == plugin.CHANNEL_LOGIN_REQUIRED:
        if cookie_present:
            raise SoopAuthError(
                "SOOP 已发送 Cookie，但频道仍要求登录或观看权限；请确认 Cookie 有效且账号已完成房间要求的认证"
            )
        raise SoopAuthError(
            "SOOP 需要有效登录或观看权限；19+ 房间要求已完成成人认证的账号"
        )
    if result != plugin.CHANNEL_RESULT_OK:
        return False

    plugin.id = room_id
    plugin.author = author
    plugin.title = title
    if room_id and rmd and cdn and isinstance(presets, list):
        plugin.magekit_soop_hint = {
            "checked_at": time.time(), "channel": channel, "broadcast": broadcast,
            "room_id": room_id, "author": author, "title": title,
            "rmd": rmd, "cdn": cdn, "broadcast_password": broadcast_password,
            "presets": presets,
        }
    return bool(room_id and rmd)


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


class RoomPageMetadata(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.images = []

    def handle_starttag(self, tag, attrs):
        if tag.lower() != "meta":
            return
        values = {key.lower(): value for key, value in attrs if key and value}
        name = (values.get("property") or values.get("name") or "").lower()
        if name in ("og:image", "og:image:url", "og:image:secure_url",
                    "twitter:image", "twitter:image:src"):
            image = values.get("content", "").strip()
            if image:
                self.images.append(image)


def huya_cover_url(session, url):
    """Fetch Huya's live screenshot from its public room metadata API."""
    parsed = urlparse(url)
    room_id = parsed.path.rstrip("/").rsplit("/", 1)[-1]
    if not room_id.isdigit():
        return None

    response = None
    try:
        response = session.http.get(
            "https://mp.huya.com/cache.php",
            params={"m": "Live", "do": "profileRoom", "roomid": room_id, "showSecret": "0"},
            timeout=8,
            headers={
                "User-Agent": "ios/7.830 (ios 17.0; ; iPhone 15 (A2846/A3089/A3090/A3092))",
                "xweb_xhr": "1",
                "Referer": "https://servicewechat.com/wx74767bf0b684f7d3/301/page-frame.html",
                "Accept-Language": "zh-CN,zh;q=0.9",
                "Accept": "application/json",
            },
        )
        if response.status_code >= 400:
            return None
        payload = response.json()
        data = payload.get("data") or {}
        live = data.get("liveData") or {}
        profile = data.get("profileInfo") or {}
        image = (
            live.get("screenshot")
            or live.get("screenShot")
            or profile.get("avatar")
            or profile.get("avatar180")
        )
        if not isinstance(image, str) or not image.strip():
            return None
        image = image.strip()
        if image.startswith("//"):
            image = "https:" + image
        elif not image.startswith(("http://", "https://")):
            image = "https://" + image.lstrip("/")
        image_parts = urlparse(image)
        trusted_host = any(
            matching_domain(image_parts.hostname or "", domain)
            for domain in ("huya.com", "msstatic.com")
        )
        if image_parts.scheme in ("http", "https") and trusted_host:
            return image
    except Exception:
        # Cover lookup is optional; a platform API hiccup must not fail the probe.
        return None
    finally:
        if response is not None:
            try:
                response.close()
            except Exception:
                pass
    return None


def page_cover_url(session, url):
    """Read a public room-page share image through the plugin's cookie-aware session."""
    response = None
    try:
        response = session.http.get(
            url,
            timeout=3,
            headers={"Accept": "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1"},
            stream=True,
        )
        if response.status_code >= 400:
            return None
        content = bytearray()
        for chunk in response.iter_content(chunk_size=64 * 1024):
            if not chunk:
                continue
            content.extend(chunk[:512_000 - len(content)])
            if len(content) >= 512_000:
                break
        metadata = RoomPageMetadata()
        encoding = getattr(response, "encoding", None) or "utf-8"
        metadata.feed(content.decode(encoding, errors="replace"))
        base_url = getattr(response, "url", None) or url
        for image in metadata.images:
            image = urljoin(base_url, image)
            parsed = urlparse(image)
            if parsed.scheme in ("http", "https") and parsed.hostname:
                return image
    except Exception:
        # Cover metadata is optional; it must never turn a usable stream into a failed probe.
        return None
    finally:
        if response is not None:
            try:
                response.close()
            except Exception:
                pass
    return None


def cache_page_cover(session, page_url, cache_path, plugin_name=None):
    image_url = huya_cover_url(session, page_url) if plugin_name == "huya" else None
    image_url = image_url or page_cover_url(session, page_url)
    cached = Path(cache_path) if cache_path else None
    temporary = None
    response = None
    try:
        if image_url and cached is not None:
            cached.parent.mkdir(parents=True, exist_ok=True)
            temporary = cached.with_name(f"{cached.name}.{os.getpid()}.tmp")
            response = session.http.get(
                image_url,
                timeout=5,
                headers={
                    "Referer": page_url,
                    "Accept": "image/avif,image/webp,image/apng,image/*,*/*;q=0.8",
                },
                stream=True,
            )
            if response.status_code < 400:
                length = response.headers.get("Content-Length")
                if not length or int(length) <= 8 * 1024 * 1024:
                    total = 0
                    prefix = bytearray()
                    with temporary.open("wb") as output:
                        for chunk in response.iter_content(chunk_size=64 * 1024):
                            if not chunk:
                                continue
                            total += len(chunk)
                            if total > 8 * 1024 * 1024:
                                raise ValueError("room cover exceeds cache limit")
                            if len(prefix) < 16:
                                prefix.extend(chunk[:16 - len(prefix)])
                            output.write(chunk)

                    known_image = (
                        prefix.startswith(b"\x89PNG\r\n\x1a\n")
                        or prefix.startswith(b"\xff\xd8\xff")
                        or prefix.startswith((b"GIF87a", b"GIF89a"))
                        or (prefix.startswith(b"RIFF") and prefix[8:12] == b"WEBP")
                    )
                    if known_image and total > 0:
                        os.replace(temporary, cached)
                        return str(cached)
    except Exception:
        # Cover download failures are non-fatal; keep using a prior local copy or the URL.
        pass
    finally:
        if response is not None:
            try:
                response.close()
            except Exception:
                pass
        if temporary is not None:
            try:
                temporary.unlink(missing_ok=True)
            except OSError:
                pass

    if cached is not None:
        try:
            if cached.is_file() and cached.stat().st_size > 0:
                return str(cached)
        except OSError:
            pass
    return image_url


def probe(request):
    session, name, plugin = resolve(request)
    try:
        status_only = name == "soop" and request.get("status_only", False)
        if status_only:
            live = soop_is_live(plugin)
            streams = {}
        else:
            streams = plugin_streams(name, plugin)
            live = bool(streams)
        parsed = urlparse(request["url"])
        room_id = str(plugin.id or parsed.path.rstrip("/").rsplit("/", 1)[-1] or parsed.hostname)[:256]
        extra = {"streamlink_plugin": name, "streamlink_qualities": list(streams)[:100]}
        if status_only and live and hasattr(plugin, "magekit_soop_hint"):
            extra["soop_hint"] = plugin.magekit_soop_hint
        room = dict(room_id=room_id, anchor_name=str(plugin.author or room_id)[:256],
                    title=str(plugin.title or room_id)[:512], status="Live" if live else "Offline",
                    start_time=None, viewer_count=None,
                    cover_url=cache_page_cover(
                        session, request["url"], request.get("cover_cache_path"), name
                    )
                    if request.get("fetch_cover", True) and not status_only else None,
                    extra=extra)
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


def prepare_soop(request):
    session, name, plugin = resolve(request)
    try:
        if name != "soop":
            return UNSUPPORTED
        quality = request.get("config", {}).get("quality", "Original")
        stream = soop_selected_stream(plugin, quality, hint=request.get("soop_hint"))
        prepared = getattr(plugin, "magekit_prepared", None)
        if stream is None or prepared is None:
            emit(kind="error", message="SOOP 当前没有可用直播流")
            return OFFLINE
        emit(kind="prepared", prepared=prepared)
        return 0
    finally:
        session.http.close()


def classify(error):
    from streamlink.exceptions import NoPluginError
    if isinstance(error, SoopAuthError):
        return AUTH, str(error)
    if isinstance(error, SoopResolutionTimeout):
        return TIMEOUT, "SOOP 直播授权解析超过 20 秒；请检查代理、网络或稍后重试"
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
    session = None
    source = None
    try:
        session, name, plugin = resolve(request)
        quality = request.get("config", {}).get("quality", "Original")
        source_stream = (
            soop_selected_stream(plugin, quality, request.get("soop_quality_offset", 0),
                                 request.get("soop_hint"))
            if name == "soop"
            else choose_stream(plugin_streams(name, plugin), quality)
        )
        if source_stream is None:
            return OFFLINE
        if name == "soop":
            selected_quality = getattr(plugin, "magekit_selected_quality", None)
            if selected_quality is not None:
                print("MAGEKIT_QUALITY:" + json.dumps(selected_quality, ensure_ascii=True),
                      file=sys.stderr, flush=True)
            print("MAGEKIT_STAGE:selected", file=sys.stderr, flush=True)
        source = source_stream.open()
        if name == "soop":
            print("MAGEKIT_STAGE:opened", file=sys.stderr, flush=True)
        while True:
            chunk = source.read(16 * 1024)
            if not chunk:
                return 0
            sys.stdout.buffer.write(chunk)
            sys.stdout.buffer.flush()
    finally:
        if source is not None:
            source.close()
        if session is not None:
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


def worker_command():
    # Re-execute the managed script by path. Embedding its source in `python -c`
    # exceeds Windows' process command-line limit as the worker grows.
    return [sys.executable, "-I", "-u", str(Path(__file__).resolve())]


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


def safe_recording_error(error, stage):
    """Expose useful local failure details without leaking URLs, cookies, or paths."""
    if isinstance(error, RuntimeError):
        return str(error)
    if isinstance(error, BrokenPipeError):
        return f"Recording pipeline failed during {stage}: Streamlink worker closed its input pipe"
    if isinstance(error, OSError):
        code = getattr(error, "winerror", None) or error.errno
        if code == 206:
            detail = "Windows path length limit exceeded (error 206)"
        elif code in (5, 13):
            detail = f"permission denied (error {code})"
        elif code == 32:
            detail = "file is locked by another process (error 32)"
        elif code == 2:
            detail = "output file or parent directory not found (error 2)"
        else:
            detail = f"I/O error (code {code})"
        return f"Recording pipeline failed during {stage}: {detail}"
    return f"Recording pipeline failed during {stage} ({type(error).__name__})"


def record_direct_ts(request):
    """Directly save SOOP's MPEG-TS segments through its plugin stream object."""
    config = request["config"]
    output = Path(request["output"])
    stop = threading.Event()

    def control():
        # Request JSON has already been consumed. The only following input is
        # a stop signal (or EOF). Use the raw FD so a daemon cannot hold the
        # BufferedReader lock while Python exits on Windows.
        try:
            os.read(sys.stdin.fileno(), 1)
        except OSError:
            pass
        stop.set()

    threading.Thread(target=control, daemon=True).start()
    started = time.monotonic()
    last_size = 0
    last_report = started
    total_input = [0]
    selected_quality = [None]
    source = None
    terminal, message = "completed", None
    maximum = config.get("max_duration")
    timeout = max(20, int(config.get("timeout", 30)))
    retries = max(0, min(100, int(config.get("retry_count", 3))))
    first_media_deadline = started + min(90, max(65, timeout * 2 + 15))
    stage = "open output file"

    def progress(state):
        nonlocal last_size, last_report
        now = time.monotonic()
        size = output.stat().st_size if output.exists() else 0
        speed = max(0, int((size - last_size) / max(0.01, now - last_report)))
        last_size, last_report = size, now
        emit(kind="progress", status=state, duration=int(now - started), size=size, speed=speed,
             quality=selected_quality[0])

    try:
        stage = "create output directory"
        output.parent.mkdir(parents=True, exist_ok=True)
        stage = (
            f"open output file (parent exists={output.parent.is_dir()}, "
            f"target exists={output.exists()})"
        )
        with output.open("ab") as target:
            for attempt in range(retries + 1):
                if stop.is_set():
                    terminal = "stopped"
                    break
                if maximum is not None and time.monotonic() - started >= maximum:
                    break

                stage = "report connection progress"
                progress("connecting")
                attempt_input = total_input[0]
                # 明确选原画时始终重试原画，不因首片慢而静默降档。
                want_best = config.get("quality") in ("Original", "Blue")
                quality_offset = 1 if total_input[0] == 0 and attempt > 0 and not want_best else 0
                hint = request.get("soop_hint")
                if attempt > 0 and isinstance(hint, dict):
                    hint = dict(hint)
                    hint.pop("prepared", None)
                used_prepared = isinstance(hint, dict) and isinstance(hint.get("prepared"), dict)
                request_pull = dict(request, mode="pull", soop_quality_offset=quality_offset,
                                    soop_hint=hint)
                stage = "start Streamlink source process"
                source = command(
                    worker_command(),
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
                source_process = source
                source_started = time.monotonic()
                source_stage = ["resolving", source_started]
                source_messages = []

                def drain_source_errors():
                    try:
                        for line in source_process.stderr:
                            message = line.decode("utf-8", errors="replace").strip()
                            if message in ("MAGEKIT_STAGE:selected", "MAGEKIT_STAGE:opened"):
                                source_stage[:] = [message.rsplit(":", 1)[-1], time.monotonic()]
                            elif message.startswith("MAGEKIT_QUALITY:"):
                                try:
                                    quality_event = json.loads(message[len("MAGEKIT_QUALITY:"):])
                                except (json.JSONDecodeError, ValueError):
                                    continue
                                if isinstance(quality_event, dict):
                                    name = quality_event.get("name")
                                    label = quality_event.get("label")
                                    if isinstance(name, str) and isinstance(label, str):
                                        cache_label = (
                                            " [预热流]" if quality_event.get("prefetched")
                                            else " [复用房态]" if quality_event.get("cached_metadata")
                                            else ""
                                        )
                                        selected_quality[0] = f"{label[:64]} ({name[:64]}){cache_label}"
                            elif message.startswith("MAGEKIT_ERROR:"):
                                try:
                                    event = json.loads(message[len("MAGEKIT_ERROR:"):])
                                except (json.JSONDecodeError, ValueError):
                                    continue
                                if (isinstance(event, dict)
                                        and event.get("code") in (AUTH, UNSUPPORTED, FAILED, TIMEOUT)
                                        and len(source_messages) < 2):
                                    detail = event.get("message")
                                    if isinstance(detail, str):
                                        source_messages.append(detail[:512])
                    except (OSError, ValueError):
                        pass

                stderr_thread = threading.Thread(target=drain_source_errors, daemon=True)
                stderr_thread.start()
                stage = "send request to Streamlink source process"
                source.stdin.write((json.dumps(request_pull) + "\n").encode())
                source.stdin.close()
                done = threading.Event()
                pump_errors = []
                last_data = [time.monotonic()]

                def pump():
                    try:
                        # 无缓冲读取，避免首批 TS 数据在 64 KiB 缓冲区内滞留。
                        while chunk := os.read(source_process.stdout.fileno(), 16 * 1024):
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
                stage = "read media stream"
                while not done.wait(0.25):
                    now = time.monotonic()
                    if now - last_report >= 1:
                        progress("recording" if total_input[0] > attempt_input else "connecting")
                    if stop.is_set() or (maximum is not None and now - started >= maximum):
                        terminal = "stopped" if stop.is_set() else "completed"
                        interrupted = True
                        break
                    # 授权和播放列表会占用较长时间；流对象打开后再计首包超时。
                    if total_input[0] == attempt_input:
                        phase, phase_started = source_stage
                        initial_timeout = max(timeout, 30)
                        if phase == "resolving":
                            timed_out = now - source_started >= max(30, min(45, timeout + 10))
                        else:
                            timed_out = now - phase_started >= initial_timeout
                    else:
                        timed_out = now - last_data[0] >= timeout
                    if total_input[0] == 0 and now >= first_media_deadline:
                        timed_out = True
                    if timed_out:
                        break

                stage = "collect Streamlink source result"
                code = source.poll()
                if done.is_set() and code is None:
                    try:
                        code = source.wait(timeout=1)
                    except subprocess.TimeoutExpired:
                        pass
                terminate(source)
                stderr_thread.join(timeout=3)
                if stderr_thread.is_alive():
                    source.stderr.close()
                    stderr_thread.join(timeout=1)
                source_message = " ".join(source_messages)
                if not done.wait(5):
                    raise RuntimeError("Media stream pump failed to stop")
                pump_thread.join(timeout=1)
                source = None

                if interrupted:
                    break
                if pump_errors:
                    raise RuntimeError("Media stream pump failed; existing output is preserved")
                if code in (AUTH, UNSUPPORTED, TIMEOUT):
                    stale_prepared = (used_prepared and total_input[0] == 0
                                      and code in (AUTH, TIMEOUT) and attempt < retries)
                    if not stale_prepared:
                        raise RuntimeError(source_message or "Stream access/plugin error; check login permissions and plugin version")
                if code == OFFLINE and total_input[0] == attempt_input and attempt > 0:
                    raise RuntimeError("SOOP 当前没有可用直播流，房间可能已下播或所选清晰度授权失败")
                if total_input[0] > 0 and code in (0, OFFLINE):
                    break
                if total_input[0] == 0 and time.monotonic() + 20 >= first_media_deadline:
                    raise RuntimeError("SOOP 连接超时：所选清晰度未收到媒体片段")
                if total_input[0] == 0 and attempt >= min(retries, 1):
                    raise RuntimeError(source_message or "SOOP 已重试所选清晰度，仍未收到媒体片段；请检查直播播放权限或网络")
                if attempt == retries:
                    raise RuntimeError(source_message or "Stream ended/stalled and the reconnect budget was exhausted")

                stage = "wait before reconnect"
                progress("connecting")
                until = time.monotonic() + min(30, 2 ** attempt)
                while time.monotonic() < until and not stop.wait(0.25):
                    if maximum is not None and time.monotonic() - started >= maximum:
                        break

        if total_input[0] == 0 and terminal != "stopped":
            raise RuntimeError("No media was recorded")
    except Exception as error:
        terminal = "error"
        message = safe_recording_error(error, stage)
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
    if config.get("format") == "ts" and request.get("direct_ts"):
        return record_direct_ts(request)
    output = Path(request["output"])
    ffmpeg = request["ffmpeg"]
    stop = threading.Event()
    def control():
        # EOF also means the Rust owner went away. Never leave a detached recording running.
        try:
            os.read(sys.stdin.fileno(), 1)
        except OSError:
            pass
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
            source = command(worker_command(),
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
            if code in (AUTH, UNSUPPORTED, TIMEOUT):
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
    # Rust sends JSON as UTF-8 bytes. Windows decodes redirected stdin with the
    # active code page by default, which can corrupt Korean/Chinese output paths.
    request = json.loads(sys.stdin.buffer.readline().decode("utf-8"))
    mode = request.get("mode", "probe")
    try:
        if mode == "record":
            return record(request)
        if mode == "pull":
            return pull(request)
        if mode == "prepare":
            return prepare_soop(request)
        probe(request)
        return 0
    except Exception as error:
        code, message = classify(error)
        if mode == "pull":
            # 父进程只读取媒体 stdout；错误摘要走 stderr，避免污染 TS 数据。
            print("MAGEKIT_ERROR:" + json.dumps({"code": code, "message": message}, ensure_ascii=True),
                  file=sys.stderr, flush=True)
        else:
            emit(kind="unsupported" if code == UNSUPPORTED else "error", message=message)
        return code


if __name__ == "__main__":
    sys.exit(main())
