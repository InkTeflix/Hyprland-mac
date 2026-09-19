#!/usr/bin/env bash
set -euo pipefail

CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
ROOT="${CONFIG_HOME}/hypr-mac"
CURRENT="${ROOT}/wallpaper-state/current"
CONFIG="${ROOT}/matugen/config.toml"

# Matugen is optional.
# If it is missing, leave the last generated palette untouched.
if ! command -v matugen >/dev/null 2>&1; then
    printf '%s\n' \
        'matugen not available; keeping the existing generated palette.' >&2
    exit 0
fi

# Do not break wallpaper handling if the current link is unavailable.
if [[ ! -e "${CURRENT}" ]]; then
    printf '%s\n' \
        'current wallpaper link is unavailable; keeping the existing palette.' >&2
    exit 0
fi

mkdir -p "${ROOT}/matugen/generated"

# Keep Spotlight in the light/Tahoe visual mode while deriving its colors
# from the current wallpaper.
exec matugen \
    -c "${CONFIG}" \
    image "${CURRENT}" \
    -m light
