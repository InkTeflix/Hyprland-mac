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

# Initial render
check_finder

# Persistent connection loop handles broken pipes smoothly
while true; do
    if [[ -S "$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock" ]]; then
        socat -U - "UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock" 2>/dev/null | while read -r line; do
            case "$line" in 
                openwindow*|closewindow*|movewindow*|changelorkspace*) 
                    check_finder 
                    ;; 
            esac
        done
    fi
    # If the pipe breaks or the socket resets, pause momentarily and re-establish connection
    sleep 0.2
done
