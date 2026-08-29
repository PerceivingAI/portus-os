# PortusOS live graphical-session entry.
# The live account is local-console oriented; SSH must never trigger X11.
if [ -z "${DISPLAY:-}" ] && [ -z "${SSH_CONNECTION:-}" ] && [ "$(tty 2>/dev/null || true)" = "/dev/tty1" ]; then
    if command -v dbus-run-session >/dev/null 2>&1 && command -v startx >/dev/null 2>&1; then
        exec dbus-run-session -- startx
    fi
fi
