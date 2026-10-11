#!/usr/bin/env bash
set -euo pipefail

# -----------------------------------------------------------------------------
# hypr-mac Matugen wallpaper watcher
# -----------------------------------------------------------------------------
#
# Event-driven and idle-friendly:
#   wallpaper-state/current changes
#       -> generate-colors.sh
#   awww daemon socket appears
#       -> generate-colors.sh
#
# While idle, inotifywait blocks in the kernel waiting for filesystem events.
# There is no timer/polling loop.
#
# NOTE: controller.sock is a command socket, not an event/notification socket;
# it does not change when a client sends `wpp set`, so watching it would not
# wake us up reliably. The stable `current` symlink is the correct event source
# for wallpaper changes made through the wpp controller.
# -----------------------------------------------------------------------------

CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
ROOT="${CONFIG_HOME}/hypr-mac"
STATE_DIR="${ROOT}/wallpaper-state"
CURRENT="${STATE_DIR}/current"
GENERATOR="${ROOT}/matugen/generate-colors.sh"

# awww 0.12.x creates:
#   ${XDG_RUNTIME_DIR}/${WAYLAND_DISPLAY}-awww-daemon.<namespace>.socket
# and falls back to /tmp/awww when XDG_RUNTIME_DIR is unavailable.
if [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
    AWWW_WATCH_DIR="${XDG_RUNTIME_DIR}"
else
    AWWW_WATCH_DIR="/tmp/awww"
    mkdir -p "${AWWW_WATCH_DIR}"
fi

if ! command -v inotifywait >/dev/null 2>&1; then
    printf '%s\n' \
        'inotifywait is required; install the inotify-tools package.' >&2
    exit 1
fi

if ! command -v flock >/dev/null 2>&1; then
    printf '%s\n' \
        'flock is required; it is normally provided by util-linux.' >&2
    exit 1
fi

if [[ ! -x "${GENERATOR}" ]]; then
    printf 'Matugen generator is missing or not executable: %s\n' \
        "${GENERATOR}" >&2
    exit 1
fi

if [[ ! -d "${STATE_DIR}" ]]; then
    printf 'Wallpaper state directory is missing: %s\n' \
        "${STATE_DIR}" >&2
    exit 1
fi

# Prevent duplicate watcher instances.
LOCK_ROOT="${XDG_RUNTIME_DIR:-${HOME}/.cache}"
mkdir -p "${LOCK_ROOT}"
LOCK_FILE="${LOCK_ROOT}/hypr-mac-matugen-watch.lock"
exec 9>"${LOCK_FILE}"
if ! flock -n 9; then
    exit 0
fi

run_generator() {
    printf '[matugen-watch] generating colors\n' >&2
    if ! "${GENERATOR}"; then
        printf '[matugen-watch] color generation failed\n' >&2
        return 1
    fi
    printf '[matugen-watch] color generation complete\n' >&2
}

# Always reconcile once when the watcher starts. This covers awww already
# running before the watcher and also makes manual startup deterministic.
run_generator || true

# Watch the directory containing the stable current symlink and the awww
# runtime directory. Both are filesystem event sources; neither is polled.
inotifywait \
    --monitor \
    --quiet \
    --event create,moved_to,close_write \
    --format '%e|%w%f' \
    "${STATE_DIR}" "${AWWW_WATCH_DIR}" |
while IFS='|' read -r events path; do
    case "${path}" in
        "${CURRENT}")
            case "${events}" in
                *CREATE*|*MOVED_TO*|*CLOSE_WRITE*)
                    # The controller replaces the symlink atomically in the
                    # normal wpp path. Give that replacement a moment to settle.
                    sleep 0.20
                    run_generator || true
                    ;;
            esac
            ;;

        "${AWWW_WATCH_DIR}"/*-awww-daemon*.socket)
            case "${events}" in
                *CREATE*|*MOVED_TO*)
                    run_generator || true
                    ;;
            esac
            ;;
    esac
done
