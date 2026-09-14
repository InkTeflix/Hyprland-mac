#!/usr/bin/env bash

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

TARGET_CONFIG_DIR="$HOME/.config/hypr-mac"
MANIFEST_FILE="$TARGET_CONFIG_DIR/.manifest"

if [ "$EUID" -eq 0 ]; then
    echo -e "${RED}[!] Error: Do not run this script with sudo or root privileges.${NC}"
    exit 1
fi

show_help() {
    echo -e "${BLUE}=== Hyprland-Mac Uninstallation Wizard ===${NC}"
    echo "Purpose: Reads the environment deployment manifest to safely and completely"
    echo "         purge user profile files, cache files, and system session records."
    echo ""
    echo "Usage: ./uninstall.sh [OPTIONS]"
    echo ""
    echo "Options & Mode Flags:"
    echo "  (None)                Standard uninstallation (asks before removing active configuration)."
    echo "  -p, --preserve-bkps   Wipe the active environment but leave older backups untouched."
    echo "  -a, --purge-all       Complete wipe. Deletes active files AND all historical backup folders."
    echo "  -s, --silent          Bypass the interactive prompt guard (runs completely unattended)."
    echo "  -h, --help            Show this uninstallation manual document window."
    exit 0
}

PURGE_MODE="standard"
SILENT_MODE=false

while [[ "$#" -gt 0 ]]; do
    case "$1" in
        -p|--preserve-bkps|--preserve-backups) PURGE_MODE="preserve"; shift ;;
        -a|--purge-all) PURGE_MODE="all"; shift ;;
        -s|--silent) SILENT_MODE=true; shift ;;
        -h|--help) show_help ;;
        *) echo -e "${RED}Unknown parameter: $1${NC}"; show_help; exit 1 ;;
    esac
done

echo -e "${BLUE}=== Hyprland-Mac Automation Removal Engine ===${NC}\n"

if [ ! -f "$MANIFEST_FILE" ]; then
    echo -e "${YELLOW}[!] Warning: Deployed installation manifest file could not be found at:"
    echo -e "    $MANIFEST_FILE"
    echo -e "The script will attempt a standard folder purge instead.${NC}\n"
fi

if [ "$SILENT_MODE" = false ]; then
    echo -e "${RED}[!] WARNING: You are about to completely remove Hyprland-Mac from your system.${NC}"
    if [ "$PURGE_MODE" = "all" ]; then
        echo -e "${RED}[!] CRITICAL: This will also permanently delete ALL matching backup directories!${NC}"
    fi
    echo -n "Are you sure you want to proceed? [y/N]: "
    read -r user_confirm

    user_confirm_lower=$(echo "$user_confirm" | tr '[:upper:]' '[:lower:]')
    if [[ "$user_confirm_lower" != "y" && "$user_confirm_lower" != "yes" ]]; then
        echo -e "${GREEN}[*] Uninstallation aborted by user.${NC}"
        exit 0
    fi
fi

echo -e "\n${BLUE}[*] Beginning system environment purge routine...${NC}"

if [ -f "$MANIFEST_FILE" ]; then
    sed -n '/DEPLOYED_FILES:/,$p' "$MANIFEST_FILE" | tail -n +2 | while read -r target_path; do
        [ -z "$target_path" ] && continue
        if [ -e "$target_path" ] || [ -L "$target_path" ]; then
            if [[ "$target_path" == /usr/share/* ]]; then
                echo -e "  [Removing System Entry] $target_path"
                sudo rm -f "$target_path"
            else
                echo -e "  [Removing User Path]   $target_path"
                rm -rf "$target_path"
            fi
        fi
    done
fi

if [ -d "$TARGET_CONFIG_DIR" ]; then
    echo -e "  [Cleaning Residuals]   $TARGET_CONFIG_DIR"
    rm -rf "$TARGET_CONFIG_DIR"
fi

if [ "$PURGE_MODE" = "all" ]; then
    echo -e "\n${YELLOW}[*] Searching for historical profile backups inside ~/.config/...${NC}"
    find "${HOME}/.config" -maxdepth 1 -type d -name "hypr-mac_backup_*" | while read -r backup_dir; do
        if [ -d "$backup_dir" ]; then
            echo -e "  [Purging Backup Folder] $backup_dir"
            rm -rf "$backup_dir"
        fi
    done
elif [ "$PURGE_MODE" = "preserve" ]; then
    echo -e "\n${GREEN}[✓] Mode (-p): All historical backup directories have been safely preserved.${NC}"
fi

echo -e "\n${GREEN}[✓] Hyprland-Mac environment successfully purged from the machine!${NC}"
