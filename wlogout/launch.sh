#!/usr/bin/env bash

TEMPLATE="$HOME/.config/hypr-mac/wlogout/layout.template"
TARGET="$HOME/.config/hypr-mac/wlogout/layout"

# 1. Check for hyprshutdown installation dependencies
if command -v hyprshutdown &> /dev/null; then
    export REBOOT_ACTION="hyprshutdown --post-cmd 'systemctl reboot'"
    export POWEROFF_ACTION="hyprshutdown --post-cmd 'systemctl poweroff'"
else
    export REBOOT_ACTION="systemctl reboot"
    export POWEROFF_ACTION="systemctl poweroff"
fi

# 2. Check for adequate swap configurations
ACTIVE_RAM=$(awk '/Active:/ {print $2}' /proc/meminfo)
TOTAL_SWAP=$(awk '/SwapTotal:/ {print $2}' /proc/meminfo)

HIBERNATE_BLOCK='{
    "label" : "hibernate",
    "action" : "systemctl hibernate",
    "text" : "Hibernate
     [ H ]",
    "keybind" : "h"
}'

QUIT_BLOCK='{
    "label" : "quit",
    "action" : "pkill wlogout",
    "text" : "Quit
 [ Q ]",
    "keybind" : "q"
}'

if [ "$TOTAL_SWAP" -gt 0 ] && [ "$TOTAL_SWAP" -ge "$ACTIVE_RAM" ]; then
    export LAST_OPTION="$HIBERNATE_BLOCK"
else
    export LAST_OPTION="$QUIT_BLOCK"
fi

# 3. Compile variables into the production layout structure
envsubst < "$TEMPLATE" > "$TARGET"

# 4. Trigger wlogout configuration interface
wlogout --layout "$TARGET" --css "$HOME/.config/hypr-mac/wlogout/style.css"
