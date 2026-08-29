#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

python_cmd=
if command -v python3 >/dev/null 2>&1; then
    python_cmd=python3
elif command -v python >/dev/null 2>&1; then
    python_cmd=python
else
    printf '%s\n' 'PortusOS build bootstrap BLOCK: Python 3.10+ is required; see BUILD_REQUIREMENTS.md' >&2
    exit 127
fi

if ! "$python_cmd" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 10) else 1)' >/dev/null 2>&1; then
    printf '%s\n' 'PortusOS build bootstrap BLOCK: Python 3.10+ is required; see BUILD_REQUIREMENTS.md' >&2
    exit 127
fi

if ! command -v git >/dev/null 2>&1; then
    printf '%s\n' 'PortusOS build bootstrap BLOCK: Git is required before a traceable build run can be allocated; see BUILD_REQUIREMENTS.md' >&2
    exit 127
fi

exec "$python_cmd" -B scripts/build_iteration.py "$@"
