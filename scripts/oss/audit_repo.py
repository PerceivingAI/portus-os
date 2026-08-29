#!/usr/bin/env python3
"""Secret/private-data and repository-hygiene audit for current tree or Git history.

The scanner reports finding type + affected path/object only. It never emits the
matched secret/private value.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT = ROOT / "artifacts" / "oss"
MAX_TEXT_BYTES = 4 * 1024 * 1024
MAX_HISTORY_BLOB = 4 * 1024 * 1024
SEVERITY_ORDER = {"INFO": 0, "LOW": 1, "MEDIUM": 2, "HIGH": 3, "BLOCKER": 4}

SECRET_PATTERNS = [
    ("private-key", "BLOCKER", re.compile(rb"-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----[\s\S]{32,}?-----END (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----")),
    ("aws-access-key", "BLOCKER", re.compile(rb"AKIA[0-9A-Z]{16}")),
    ("github-token", "BLOCKER", re.compile(rb"gh[pousr]_[A-Za-z0-9]{20,}")),
    ("google-api-key", "BLOCKER", re.compile(rb"AIza[0-9A-Za-z_-]{30,}")),
    ("slack-token", "BLOCKER", re.compile(rb"xox[baprs]-[A-Za-z0-9-]{10,}")),
    ("openai-style-key", "HIGH", re.compile(rb"\bsk-[A-Za-z0-9_-]{24,}\b")),
    ("jwt", "HIGH", re.compile(rb"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b")),
]

ASSIGNMENT = re.compile(
    rb"(?i)\b(?:api[_-]?key|access[_-]?token|refresh[_-]?token|password|passwd|client[_-]?secret|webhook[_-]?secret)\b\s*[:=]\s*[\"']?([A-Za-z0-9_./+\-=]{20,})"
)
PLACEHOLDER = re.compile(rb"(?i)(?:example|placeholder|dummy|fake|redacted|changeme|your[_-]|<[^>]+>|\$\{|0{8,}|x{8,})")
WINDOWS_USER = re.compile(rb"(?i)[A-Z]:\\Users\\([^\\\s\"']+)")
UNIX_USER = re.compile(rb"/(?:home|Users)/([A-Za-z0-9._-]+)")
ALLOWED_USER_PARTS = {b"user", b"username", b"portus", b"master", b"root", b"example", b"test", b"demo"}

RISKY_TRACKED_NAMES = {
    ".env": "HIGH",
    ".dev.vars": "HIGH",
}
RISKY_SUFFIXES = {
    ".pem": "HIGH",
    ".key": "HIGH",
    ".p12": "HIGH",
    ".pfx": "HIGH",
    ".sqlite": "MEDIUM",
    ".sqlite3": "MEDIUM",
    ".db": "MEDIUM",
    ".iso": "MEDIUM",
    ".log": "MEDIUM",
}
GENERATED_PREFIXES = ("target/", "portusos-build/work/", "portusos-build/cache/", "portusos-build/out/", "artifacts/oss/")


@dataclass(frozen=True)
class Finding:
    severity: str
    kind: str
    location: str
    object_id: str | None = None
    note: str | None = None


def git(args: list[str], *, text: bool = True, input_data=None):
    proc = subprocess.run(["git", *args], cwd=ROOT, text=text, input=input_data, capture_output=True, check=False)
    if proc.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed ({proc.returncode}): {proc.stderr.strip() if text else 'binary stderr'}")
    return proc.stdout


def safe_text(data: bytes) -> bool:
    if b"\x00" in data[:8192]:
        return False
    sample = data[:8192]
    if not sample:
        return True
    controls = sum(byte < 9 or (13 < byte < 32) for byte in sample)
    return controls / len(sample) < 0.02


def scan_payload(data: bytes, location: str, object_id: str | None = None) -> list[Finding]:
    if len(data) > MAX_TEXT_BYTES or not safe_text(data):
        return []
    findings: list[Finding] = []
    for kind, severity, pattern in SECRET_PATTERNS:
        if pattern.search(data):
            findings.append(Finding(severity, kind, location, object_id, "matched value withheld"))
    for match in ASSIGNMENT.finditer(data):
        value = match.group(1)
        if not PLACEHOLDER.search(value):
            findings.append(Finding("HIGH", "credential-assignment", location, object_id, "matched value withheld"))
            break
    for match in WINDOWS_USER.finditer(data):
        user = match.group(1).lower()
        if user not in ALLOWED_USER_PARTS:
            findings.append(Finding("MEDIUM", "personal-windows-user-path", location, object_id, "username withheld"))
            break
    for match in UNIX_USER.finditer(data):
        user = match.group(1).lower()
        if user not in ALLOWED_USER_PARTS and not user.startswith((b"<", b"{", b"$")):
            findings.append(Finding("MEDIUM", "personal-unix-user-path", location, object_id, "username withheld"))
            break
    return findings


def path_findings(path: str, *, history: bool = False, object_id: str | None = None) -> list[Finding]:
    normalized = path.replace("\\", "/")
    name = Path(normalized).name.lower()
    suffix = Path(normalized).suffix.lower()
    findings: list[Finding] = []
    if name in RISKY_TRACKED_NAMES:
        findings.append(Finding(RISKY_TRACKED_NAMES[name], "sensitive-file-name", normalized, object_id))
    if suffix in RISKY_SUFFIXES:
        findings.append(Finding(RISKY_SUFFIXES[suffix], "sensitive-or-generated-file-type", normalized, object_id))
    if any(normalized.startswith(prefix) for prefix in GENERATED_PREFIXES):
        findings.append(Finding("MEDIUM", "generated-output-tracked", normalized, object_id))
    if history and (name in {"id_rsa", "id_ed25519"} or normalized.endswith(".dev.vars")):
        findings.append(Finding("HIGH", "historical-sensitive-path", normalized, object_id))
    return findings


def dedupe(findings: list[Finding]) -> list[Finding]:
    seen = set()
    output = []
    for finding in sorted(findings, key=lambda f: (-SEVERITY_ORDER[f.severity], f.kind, f.location, f.object_id or "")):
        key = (finding.severity, finding.kind, finding.location, finding.object_id)
        if key not in seen:
            seen.add(key)
            output.append(finding)
    return output


def scan_current() -> tuple[list[Finding], dict]:
    paths = [item for item in git(["ls-files", "-z"]).split("\x00") if item]
    findings: list[Finding] = []
    binary_files = 0
    large_files = 0
    present_tracked = 0
    deleted_tracked = 0
    for path in paths:
        file_path = ROOT / path
        if not file_path.is_file():
            deleted_tracked += 1
            continue
        present_tracked += 1
        findings.extend(path_findings(path))
        data = file_path.read_bytes()
        if len(data) > MAX_TEXT_BYTES:
            large_files += 1
            continue
        if not safe_text(data):
            binary_files += 1
            continue
        findings.extend(scan_payload(data, path))

    untracked = [item for item in git(["ls-files", "--others", "--exclude-standard", "-z"]).split("\x00") if item]
    ignored = [line[3:] for line in git(["status", "--short", "--ignored"]).splitlines() if line.startswith("!! ")]
    for path in untracked:
        findings.extend(path_findings(path))
        file_path = ROOT / path
        if not file_path.is_file():
            continue
        data = file_path.read_bytes()
        if len(data) <= MAX_TEXT_BYTES and safe_text(data):
            findings.extend(scan_payload(data, path))
    stats = {
        "tracked_files": len(paths),
        "tracked_files_present": present_tracked,
        "tracked_files_deleted_in_worktree": deleted_tracked,
        "untracked_nonignored": len(untracked),
        "ignored_entries_present": ignored,
        "binary_tracked_files": binary_files,
        "large_tracked_files_skipped_content_scan": large_files,
    }
    return dedupe(findings), stats


def history_objects() -> tuple[list[tuple[str, str]], dict[str, list[str]]]:
    raw = git(["rev-list", "--objects", "--all"])
    ordered: list[tuple[str, str]] = []
    paths_by_oid: dict[str, list[str]] = {}
    for line in raw.splitlines():
        if not line:
            continue
        oid, _, path = line.partition(" ")
        if oid not in paths_by_oid:
            ordered.append((oid, path))
            paths_by_oid[oid] = []
        if path and path not in paths_by_oid[oid]:
            paths_by_oid[oid].append(path)
    return ordered, paths_by_oid


def scan_history() -> tuple[list[Finding], dict]:
    ordered, paths_by_oid = history_objects()
    findings: list[Finding] = []
    blobs = 0
    large_blobs = 0
    binary_blobs = 0

    proc = subprocess.Popen(
        ["git", "cat-file", "--batch"], cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    assert proc.stdin is not None and proc.stdout is not None
    try:
        for oid, first_path in ordered:
            proc.stdin.write((oid + "\n").encode("ascii"))
            proc.stdin.flush()
            header = proc.stdout.readline().decode("ascii", "replace").strip()
            parts = header.split()
            if len(parts) < 3 or parts[1] == "missing":
                continue
            _, obj_type, size_text = parts[:3]
            size = int(size_text)
            payload = proc.stdout.read(size)
            proc.stdout.read(1)
            if obj_type != "blob":
                continue
            blobs += 1
            paths = paths_by_oid.get(oid) or ([first_path] if first_path else ["<unknown>"])
            location = paths[0]
            for path in paths:
                findings.extend(path_findings(path, history=True, object_id=oid))
            if size > MAX_HISTORY_BLOB:
                large_blobs += 1
                if size > 25 * 1024 * 1024:
                    findings.append(Finding("MEDIUM", "large-historical-blob", location, oid, f"size_bytes={size}"))
                continue
            if not safe_text(payload):
                binary_blobs += 1
                continue
            findings.extend(scan_payload(payload, location, oid))
    finally:
        try:
            proc.stdin.close()
        except Exception:
            pass
        proc.wait(timeout=30)
        if proc.returncode not in (0, None):
            stderr = (proc.stderr.read() if proc.stderr else b"").decode("utf-8", "replace")
            raise RuntimeError(f"git cat-file --batch failed: {stderr.strip()}")

    stats = {
        "reachable_objects": len(ordered),
        "unique_blobs_scanned": blobs,
        "binary_blobs_skipped_content_scan": binary_blobs,
        "large_blobs_skipped_content_scan": large_blobs,
    }
    return dedupe(findings), stats


def render_markdown(report: dict) -> str:
    lines = [
        "# PortusOS OSS Repository Audit Report",
        "",
        f"- Source revision: `{report['source_revision']}`",
        f"- Scope: `{report['scope']}`",
        f"- Result: **{report['result']}**",
        f"- Findings: {len(report['findings'])}",
        "",
        "The report intentionally withholds matched secret/private values.",
        "",
    ]
    if report["findings"]:
        lines += ["## Findings", "", "| Severity | Kind | Location | Object | Note |", "| --- | --- | --- | --- | --- |"]
        for item in report["findings"]:
            lines.append(
                f"| {item['severity']} | `{item['kind']}` | `{item['location']}` | `{item.get('object_id') or ''}` | {item.get('note') or ''} |"
            )
    else:
        lines += ["## Findings", "", "No findings were detected by the automated preparatory scanner."]
    lines += ["", "## Scope statistics", "", "```json", json.dumps(report["stats"], indent=2, sort_keys=True), "```", ""]
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scope", choices=["current", "history", "both"], default="both")
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--strict", action="store_true", help="fail on any MEDIUM/HIGH/BLOCKER finding")
    args = parser.parse_args([arg for arg in sys.argv[1:] if arg.strip()])

    findings: list[Finding] = []
    stats: dict = {}
    if args.scope in ("current", "both"):
        current, current_stats = scan_current()
        findings.extend(current)
        stats["current"] = current_stats
    if args.scope in ("history", "both"):
        history, history_stats = scan_history()
        findings.extend(history)
        stats["history"] = history_stats
    findings = dedupe(findings)

    result = "pass" if not findings else "fail"
    report = {
        "schema_version": 1,
        "authority": "scripts/oss/README.md",
        "scope": args.scope,
        "source_revision": git(["rev-parse", "HEAD"]).strip(),
        "source_tree_clean": not bool(git(["status", "--short"]).strip()),
        "result": result,
        "stats": stats,
        "findings": [asdict(item) for item in findings],
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / f"repo-audit-{args.scope}.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (args.output_dir / f"repo-audit-{args.scope}.md").write_text(render_markdown(report), encoding="utf-8")

    counts = {severity: 0 for severity in SEVERITY_ORDER}
    for finding in findings:
        counts[finding.severity] += 1
    print(json.dumps({"scope": args.scope, "result": result, "findings": counts}, indent=2))
    if args.strict and any(SEVERITY_ORDER[item.severity] >= SEVERITY_ORDER["MEDIUM"] for item in findings):
        return 1
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as exc:
        print(f"repository audit failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
