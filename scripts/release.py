#!/usr/bin/env python3
"""规划 main 分支的自动发布并生成中文更新日志。

脚本不依赖第三方库。提交信息通过 Git 参数接口读取，不会拼入 shell 命令。
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib import error, parse, request


ROOT = Path(__file__).resolve().parent.parent
TAG_RE = re.compile(r"^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")
REPOSITORY_RE = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
CONVENTIONAL_RE = re.compile(
    r"^([A-Za-z][A-Za-z0-9-]*)(?:\([^()\r\n]+\))?(!)?:\s*(.+)$"
)
SKIP_RE = re.compile(r"\[(?:skip release|release skip)\]", re.IGNORECASE)
BREAKING_RE = re.compile(r"(?im)^BREAKING(?: CHANGE|-CHANGE):\s*\S")
SECTION_NAMES = {
    "feat": "新功能",
    "fix": "问题修复",
    "perf": "性能优化",
    "refactor": "代码重构",
    "docs": "文档",
    "build": "构建",
    "ci": "自动化",
    "test": "测试",
    "chore": "维护",
    "revert": "回退",
    "style": "样式",
}
SECTION_ORDER = ["破坏性变更", *SECTION_NAMES.values(), "其他更新"]


class ReleaseError(Exception):
    """无法安全计算发布计划。"""


def git(*args: str, binary: bool = False) -> str | bytes:
    result = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode:
        detail = result.stderr.decode("utf-8", "replace").strip().splitlines()
        raise ReleaseError(f"git {args[0]} 失败: {detail[0] if detail else result.returncode}")
    return result.stdout if binary else result.stdout.decode("utf-8", "replace").strip()


def tag_version(tag: str) -> tuple[int, int, int]:
    match = TAG_RE.fullmatch(tag)
    if not match:
        raise ReleaseError(f"仅支持稳定版本号 vX.Y.Z: {tag!r}")
    return tuple(map(int, match.groups()))


def stable_tags() -> list[str]:
    tags = git("for-each-ref", "--format=%(refname:short)", "refs/tags")
    return sorted(
        (tag for tag in tags.splitlines() if TAG_RE.fullmatch(tag)),
        key=tag_version,
        reverse=True,
    )


def tag_commit(tag: str) -> str:
    tag_version(tag)
    return git("rev-parse", "--verify", f"{tag}^{{commit}}")


def ensure_ancestor(tag: str, head: str) -> None:
    # merge-base 返回 1 表示 tag 位于另一条历史；此时必须终止，避免按错误基线升版。
    git("merge-base", "--is-ancestor", tag, head)


def changed_files(previous_tag: str, head: str) -> list[str]:
    if previous_tag:
        data = git(
            "diff", "--name-only", "--no-renames", "-z", previous_tag, head, "--", binary=True
        )
    else:
        data = git("ls-tree", "-r", "--name-only", "-z", head, binary=True)
    return [name.decode("utf-8", "surrogateescape") for name in data.split(b"\0") if name]


def is_documentation(filename: str) -> bool:
    return (
        filename.startswith("docs/")
        or "/" not in filename
        and (
            filename.lower().endswith((".md", ".rst"))
            or re.fullmatch(r"(?:LICENSE|NOTICE)(?:\.[^/]+)?", filename, re.IGNORECASE)
            is not None
        )
    )


def commits_since(previous_tag: str, head: str) -> list[tuple[str, str]]:
    revision = f"{previous_tag}..{head}" if previous_tag else head
    data = git("log", "--no-merges", "--reverse", "-z", "--format=%H%x00%B", revision, binary=True)
    pieces = data.split(b"\0")
    if pieces and not pieces[-1]:
        pieces.pop()
    if len(pieces) % 2:
        raise ReleaseError("无法解析 Git 提交记录")
    return [
        (pieces[index].decode("ascii"), pieces[index + 1].decode("utf-8", "replace").strip())
        for index in range(0, len(pieces), 2)
    ]


def release_kind(commits: list[tuple[str, str]]) -> str:
    has_feature = False
    for _sha, message in commits:
        subject = message.splitlines()[0] if message else ""
        conventional = CONVENTIONAL_RE.match(subject)
        if (conventional and conventional.group(2)) or BREAKING_RE.search(message):
            return "major"
        if conventional and conventional.group(1).lower() == "feat":
            has_feature = True
    return "minor" if has_feature else "patch"


def bump_version(version: tuple[int, int, int], kind: str) -> tuple[int, int, int]:
    major, minor, patch = version
    if kind == "major":
        return major + 1, 0, 0
    if kind == "minor":
        return major, minor + 1, 0
    return major, minor, patch + 1


def initial_version() -> tuple[int, int, int]:
    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    section = re.search(r"(?ms)^\[workspace\.package\]\s*\n(.*?)(?=^\[|\Z)", cargo)
    version = re.search(r'(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"\s*$', section.group(1)) if section else None
    if not version:
        raise ReleaseError("Cargo.toml 缺少 workspace.package.version")
    return tag_version("v" + version.group(1))


def github_repository() -> str:
    repository = os.getenv("GITHUB_REPOSITORY", "")
    if not repository:
        origin = git("remote", "get-url", "origin")
        match = re.search(r"github\.com[:/]([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+?)(?:\.git)?$", origin)
        repository = match.group(1) if match else ""
    if not REPOSITORY_RE.fullmatch(repository):
        raise ReleaseError("无法确定 GitHub 仓库，需设置 GITHUB_REPOSITORY")
    return repository


def release_status(tag: str) -> str:
    repository = github_repository()
    api_root = os.getenv("GITHUB_API_URL", "https://api.github.com").rstrip("/")
    url = f"{api_root}/repos/{repository}/releases/tags/{parse.quote(tag, safe='')}"
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "MageKit-release-planner",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    token = os.getenv("GH_TOKEN") or os.getenv("GITHUB_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    try:
        with request.urlopen(request.Request(url, headers=headers), timeout=30) as response:
            payload = json.load(response)
    except error.HTTPError as exc:
        if exc.code == 404:
            return "missing"
        raise ReleaseError(f"查询 GitHub Release 失败：HTTP {exc.code}") from exc
    except (error.URLError, TimeoutError, ValueError) as exc:
        raise ReleaseError(f"查询 GitHub Release 失败：{type(exc).__name__}") from exc
    if (
        not isinstance(payload, dict)
        or not isinstance(payload.get("draft"), bool)
        or not isinstance(payload.get("prerelease"), bool)
    ):
        raise ReleaseError("GitHub Release API 响应缺少 draft 或 prerelease 状态")
    if payload["draft"]:
        return "draft"
    return "prerelease" if payload["prerelease"] else "published"


def append_outputs(*, release: bool, version: str = "", previous_tag: str = "") -> None:
    values = {
        "release": "true" if release else "false",
        "version": version,
        "app_version": version.removeprefix("v") if version else "",
        "previous_tag": previous_tag,
    }
    lines = "".join(f"{key}={value}\n" for key, value in values.items())
    output = os.getenv("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8", newline="\n") as stream:
            stream.write(lines)
    else:
        print(lines, end="")


def plan_release() -> None:
    head = git("rev-parse", "HEAD")
    if os.getenv("GITHUB_SHA") and os.environ["GITHUB_SHA"] != head:
        raise ReleaseError("检出的 HEAD 与触发工作流的提交不一致")
    if os.getenv("GITHUB_REF") and os.environ["GITHUB_REF"] != "refs/heads/main":
        raise ReleaseError("自动发布仅能从 main 分支执行")

    tags = stable_tags()
    latest = tags[0] if tags else ""
    if latest:
        ensure_ancestor(latest, head)

    # 草稿或只有 tag 的版本还未发布，更新日志必须追溯到最后一个正式版本。
    baseline = ""
    latest_status = "missing"
    for tag in tags:
        status = release_status(tag)
        if tag == latest:
            latest_status = status
        if status == "published":
            baseline = tag
            break
    if baseline:
        ensure_ancestor(baseline, head)

    if latest and tag_commit(latest) == head:
        if latest_status in ("published", "prerelease"):
            append_outputs(release=False, previous_tag=baseline)
            return
        # 上传失败留下的 tag 指向同一源码时，可复用该版本号继续发布。
        append_outputs(release=True, version=latest, previous_tag=baseline)
        return

    message = git("show", "-s", "--format=%B", head)
    files = changed_files(baseline, head)
    if SKIP_RE.search(message) or not files or all(map(is_documentation, files)):
        append_outputs(release=False, previous_tag=baseline)
        return

    commits = commits_since(baseline, head)
    base = tag_version(baseline) if baseline else initial_version()
    major, minor, patch = bump_version(base, release_kind(commits))
    if latest and (major, minor, patch) <= tag_version(latest):
        major, minor, patch = bump_version(tag_version(latest), "patch")
    version = f"v{major}.{minor}.{patch}"
    append_outputs(release=True, version=version, previous_tag=baseline)


def markdown_text(value: str) -> str:
    # 保留普通和中文提交标题，同时阻止标题注入 Markdown 链接、标题或列表项。
    single_line = " ".join(value.split())[:500]
    single_line = single_line.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    return re.sub(r"([\\`*_[\]()~#!|])", r"\\\1", single_line)


def changelog(version: str, previous_tag: str, head: str, repository: str) -> str:
    groups: dict[str, list[str]] = {name: [] for name in SECTION_ORDER}
    commits = commits_since(previous_tag, head)
    for sha, message in commits:
        subject = message.splitlines()[0] if message else "（无标题提交）"
        conventional = CONVENTIONAL_RE.match(subject)
        if (conventional and conventional.group(2)) or BREAKING_RE.search(message):
            section = "破坏性变更"
        elif conventional:
            section = SECTION_NAMES.get(conventional.group(1).lower(), "其他更新")
        else:
            section = "其他更新"
        link = f"https://github.com/{repository}/commit/{sha}"
        groups[section].append(f"- {markdown_text(subject)} ([{sha[:7]}]({link}))")

    if not commits:
        groups["其他更新"].append(
            f"- 合并更新 ([{head[:7]}](https://github.com/{repository}/commit/{head}))"
        )
    compare = (
        f"与 [{previous_tag}](https://github.com/{repository}/releases/tag/{previous_tag}) 对比，"
        f"查看[完整提交](https://github.com/{repository}/compare/{previous_tag}...{version})。"
        if previous_tag else "首次发布，以下包含当前版本的提交。"
    )
    parts = [compare]
    for section in SECTION_ORDER:
        if groups[section]:
            parts.extend((f"#### {section}", "", *groups[section], ""))
    return "\n\n".join(parts[:1]) + "\n\n" + "\n".join(parts[1:]).rstrip() + "\n"


def write_notes(version: str, previous_tag: str) -> None:
    tag_version(version)
    if previous_tag:
        tag_version(previous_tag)
        if tag_version(previous_tag) >= tag_version(version):
            raise ReleaseError("previous_tag 必须早于发布版本")
    head = git("rev-parse", "HEAD")
    if previous_tag:
        ensure_ancestor(previous_tag, head)
    repository = github_repository()
    changes = changelog(version, previous_tag, head, repository)
    release_notes = f"""## MageKit {version}

### 下载

| 平台 | 文件 | 说明 |
| --- | --- | --- |
| **macOS** | `MageKit-{version}-macos-universal.dmg` | Intel 与 Apple Silicon 通用安装包 |
| **macOS** | `MageKit-{version}-macos-universal.app.zip` | 应用程序压缩包 |
| **Windows** | `MageKit-{version}-windows-x64.exe` | 独立可执行文件 |
| **Windows** | `MageKit-{version}-windows-x64-portable.zip` | 便携版 |
| **Linux** | `MageKit-{version}-linux-x64.AppImage` | AppImage |
| **Linux** | `MageKit-{version}-linux-x64.tar.gz` | 二进制压缩包 |
| **更新日志** | `MageKit-{version}-CHANGELOG.md` | 本版本中文更新日志 |

### 系统要求

- **macOS**：macOS 11.0 (Big Sur) 或更新版本
- **Windows**：Windows 10 或更新版本
- **Linux**：需要支持 Vulkan 的显卡驱动

### 更新日志

{changes}"""
    (ROOT / "RELEASE_NOTES.md").write_text(release_notes, encoding="utf-8", newline="\n")
    standalone = f"# MageKit {version} 更新日志\n\n{changes.replace('#### ', '## ')}"
    (ROOT / f"MageKit-{version}-CHANGELOG.md").write_text(
        standalone, encoding="utf-8", newline="\n"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="MageKit 自动发布版本与更新日志")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("plan", help="计算版本并写入 GITHUB_OUTPUT")
    notes = commands.add_parser("notes", help="生成 Release 说明与 changelog 附件")
    notes.add_argument("--version", required=True, help="稳定版本号，例如 v0.2.0")
    notes.add_argument("--previous-tag", default="", help="上一稳定版 tag，可留空")
    args = parser.parse_args()
    if args.command == "plan":
        plan_release()
    else:
        write_notes(args.version, args.previous_tag)


if __name__ == "__main__":
    try:
        main()
    except (ReleaseError, OSError) as exc:
        print(f"发布脚本错误：{exc}", file=sys.stderr)
        sys.exit(1)
