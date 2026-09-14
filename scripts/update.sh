#!/usr/bin/env bash

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"
TARGET_CONFIG_DIR="$HOME/.config/hypr-mac"
MANIFEST_FILE="$TARGET_CONFIG_DIR/.manifest"
REPO_URL="https://github.com/InkTeflix/Hyprland-mac.git"

show_help() {
    echo -e "${BLUE}=== Hyprland-Mac Maintenance & Update Module ===${NC}"
    echo "Purpose: Syncs repository code blocks with upstream git commits while tracking"
    echo "         and protecting user settings or configuration overrides."
    echo ""
    echo "Usage: ./update.sh [OPTIONS]"
    echo ""
    echo "Modes & Option Flags:"
    echo "  (None)            Standard codebase update (creates a readable copy backup first)."
    echo "  -r, --re-new      Complete baseline replacement profile upgrade (overwrites adjustments)."
    echo "  -n, --not-custom  Strict safety mode (updates framework dependencies, preserves user config)."
    echo "  -h, --help        Show this dynamic update system help log menu."
    exit 0
}

UPDATE_MODE="standard"
while [[ "$#" -gt 0 ]]; do
    case "$1" in
        -r|--re-new) UPDATE_MODE="renew"; shift ;;
        -n|--not-custom) UPDATE_MODE="preserve"; shift ;;
        -h|--help) show_help ;;
        *) echo -e "${RED}Unknown update parameter: $1${NC}"; show_help; exit 1 ;;
    esac
done

echo -e "${BLUE}=== Hyprland-Mac Automation Update Engine ===${NC}\n"

if [ "$UPDATE_MODE" != "renew" ] && [ -d "$TARGET_CONFIG_DIR" ]; then
    HUMAN_TIME=$(date '+%d-%b-%Y_%I-%M%p')
    BACKUP_PATH="${HOME}/.config/hypr-mac_backup_${HUMAN_TIME}"
    echo -e "${BLUE}[*] Creating timestamped verification backup:${NC}"
    echo -e "    -> $BACKUP_PATH"
    cp -r "$TARGET_CONFIG_DIR" "$BACKUP_PATH"
fi

echo -e "\n${BLUE}[*] Fetching changes from upstream repository...${NC}"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

if git -C "$REPO_ROOT" rev-parse --is-inside-work-tree &> /dev/null; then
    git -C "$REPO_ROOT" fetch origin
    LOCAL=$(git -C "$REPO_ROOT" rev-parse @)
    REMOTE=$(git -C "$REPO_ROOT" rev-parse @{u})
    
    if [ "$LOCAL" = "$REMOTE" ]; then
        echo -e "${GREEN}[✓] Update engine verifies local codebase is already up to date.${NC}"
    else
        echo -e "${YELLOW}[!] Syncing branch changes with upstream origin...${NC}"
        git -C "$REPO_ROOT" pull origin main
    fi
else
    # Fallback to fresh shallow pull if running script separated from Git directory context
    TEMP_CLONE_DIR=$(mktemp -d)
    echo -e "${YELLOW}[!] Loose execution environment. Pulling remote branch copy...${NC}"
    if git clone --depth 1 "$REPO_URL" "$TEMP_CLONE_DIR"; then
        REPO_ROOT="$TEMP_CLONE_DIR"
    else
        echo -e "${RED}[!] Error: Could not sync remote objects.${NC}"
        exit 1
    fi
fi

echo -e "\n${BLUE}[*] Rerunning system environment dependency evaluations...${NC}"
if [ -f "$SCRIPT_DIR/dependency.sh" ]; then
    "$SCRIPT_DIR/dependency.sh"
    if [ $? -ne 0 ]; then
        echo -e "${RED}[!] Core environment dependencies missing. Fix system constraints.${NC}"
        exit 1
    fi
else
    echo -e "${RED}[!] Error: dependency.sh could not be located.${NC}"
    exit 1
fi

echo -e "\n${BLUE}[*] Deploying updated configuration layout matrices...${NC}"
mkdir -p "$TARGET_CONFIG_DIR"

case "$UPDATE_MODE" in
    "renew")
        echo -e "${YELLOW}[!] Executing complete profile overwrite (-r)...${NC}"
        cp -r "$REPO_ROOT/config/." "$TARGET_CONFIG_DIR/"
        ;;
    "preserve")
        echo -e "${GREEN}[✓] Safe mode (-n): Personal settings preserved, skipping configuration updates.${NC}"
        ;;
    "standard")
        echo -e "${BLUE}[*] Running standard core code updates...${NC}"
        # Only inject missing directory matrices without replacing modified targets
        for source_item in "$REPO_ROOT"/config/*; do
            [ -e "$source_item" ] || continue
            item_name=$(basename "$source_item")
            if [ ! -e "$TARGET_CONFIG_DIR/$item_name" ]; then
                echo -e "  [Syncing Element]     $item_name"
                cp -r "$source_item" "$TARGET_CONFIG_DIR/"
            fi
        done
        ;;
esac

echo -e "\n${BLUE}[*] Re-indexing target configuration installation manifest...${NC}"
echo "# Hyprland-Mac Deployment Tracking State Manifest" > "$MANIFEST_FILE"
echo "VERSION=1.0.0 (Updated)" >> "$MANIFEST_FILE"
echo "LAST_UPDATE_DATE=$(date '+%Y-%m-%d %H:%M:%S')" >> "$MANIFEST_FILE"
echo "UPDATE_MODE=$UPDATE_MODE" >> "$MANIFEST_FILE"

echo -e "\n${GREEN}[✓] System migration update complete. Local profile files verified successfully.${NC}"
