#!/usr/bin/env bash

# 1. ISOLATE ENVIRONMENT PATHS (Preset A Icons)
export XDG_CONFIG_HOME="$HOME/.config/hypr-mac"

# Apply GSettings for GTK apps that skip config directories
if command -v gsettings &> /dev/null; then
    gsettings set org.gnome.desktop.interface icon-theme "Papirus-Dark"
fi

# 2. RUN ISOLATED DAEMONS (Mako & Waypaper)
# The '&' forces them into the background so your Lua config doesn't lock up!
pkill mako
pkill waypaper

mako --config ~/.config/hypr-mac/mako/config &
waypaper --restore --config-file ~/.config/hypr-mac/waypaper.ini &

# 3. RUN THE MATUGEN INJECTION
matugen -c ~/.config/hypr-mac/matugen/config.toml image ~/.config/hypr-mac/.cache/current_wallpaper.png &

# 4. RUN REGENERATING WAYBAR
while true; do waybar -c ~/.config/hypr-mac/waybar/config.jsonc -s ~/.config/hypr-mac/waybar/style.css; sleep 0.5; done

# 5. RUN AWWW AND LAST WALLPAPER
awww-daemon && awww img ~/.config/hypr-mac/wpp/default-light.jpg
