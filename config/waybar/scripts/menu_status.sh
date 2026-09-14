#!/usr/bin/env zsh

check_menu() {
    local active_json=$(hyprctl activewindow -j 2>/dev/null)
    local workspace_json=$(hyprctl activeworkspace -j 2>/dev/null)
    
    local window_count=$(echo "$workspace_json" | jq -r '.windows // 0')
    local active_window=""
    
    if [[ -n "$active_json" && "$active_json" != "{}" ]]; then
        active_window=$(echo "$active_json" | jq -r '.class // ""')
    fi

    if [[ "$window_count" -eq 0 || "$active_window" == "org.kde.dolphin" || "$active_window" == "dolphin" ]]; then
        echo '{"text": "File    Edit    View    Go    Window    Help", "class": "visible"}'
    else
        echo '{"text": "", "class": "hidden"}'
    fi
}

# Initial render
check_menu

# Persistent connection loop handles broken pipes smoothly
while true; do
    if [[ -S "$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock" ]]; then
        socat -U - "UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock" 2>/dev/null | while read -r line; do
            case "$line" in 
                openwindow*|closewindow*|movewindow*|activewindow*) 
                    check_menu 
                    ;; 
            esac
        done
    fi
    # If the pipe breaks or the socket resets, pause momentarily and re-establish connection
    sleep 0.2
done
