#!/usr/bin/env zsh

check_finder() {
    local workspace_json=$(hyprctl activeworkspace -j 2>/dev/null)
    local window_count=$(echo "$workspace_json" | jq -r '.windows // 0')
    
    if [[ "$window_count" -eq 0 ]]; then
        echo '{"text": "Finder", "class": "visible"}'
    else
        echo '{"text": "Finder", "class": "hidden"}'
    fi
}

check_finder

socat -U - "UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock" | while read -r line; do
    case "$line" in 
        openwindow*|closewindow*|movewindow*|changelorkspace*) 
            check_finder 
            ;; 
    esac
done
