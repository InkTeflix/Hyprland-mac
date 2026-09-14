#!/usr/bin/env bash

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"
TARGET_CONFIG_DIR="$HOME/.config/hypr-mac"
MANIFEST_FILE="$TARGET_CONFIG_DIR/.manifest"
DESKTOP_ENTRY_PATH="/usr/share/wayland-sessions/hyprland-mac.desktop"
REPO_URL="https://github.com/InkTeflix/Hyprland-mac.git"

if [ "$EUID" -eq 0 ]; then
    echo -e "${RED}[!] Error: Do not run this script with sudo or root privileges.${NC}"
    exit 1
fi

show_help() {
    echo -e "${BLUE}=== Hyprland-Mac Main Setup Wizard ===${NC}"
    echo "Purpose: Performs initialization setup, config synchronization, automated system"
    echo "         backup routines, and sets up desktop display session managers."
    echo ""
    echo "Usage: ./install.sh [OPTIONS]"
    echo ""
    echo "Modes & Option Flags:"
    echo "  (None)                Standard user interactive layout configuration deployment."
    echo "  -c, --check           Runs dependency checker engine scripts natively then exits."
    echo "  -i, --install-deps    Automated distribution software package auto-installation."
    echo "  -n, --no-desktop      Skips creating the system wayland-session login entry."
    echo "  -g, --git-pull        Forces cloning a fresh copy of the repo from GitHub."
    echo "  -h, --help            Render this clear functional help guide context manual."
    exit 0
}

RUN_CHECK_ONLY=false
AUTO_INSTALL_DEPS=false
SKIP_DESKTOP=false
RUN_GIT_CLONE=false

while [[ "$#" -gt 0 ]]; do
    case "$1" in
        -c|--check) RUN_CHECK_ONLY=true; shift ;;
        -i|--install-deps|--install-dependencies) AUTO_INSTALL_DEPS=true; shift ;;
        -n|--no-desktop|--no-desktop-entry) SKIP_DESKTOP=true; shift ;;
        -g|--git-pull|--clone) RUN_GIT_CLONE=true; shift ;;
        -h|--help) show_help ;;
        *) echo -e "${RED}Unknown parameter: $1${NC}"; show_help; exit 1 ;;
    esac
done

run_dependency_check() {
    if [ -f "$SCRIPT_DIR/dependency.sh" ]; then
        "$SCRIPT_DIR/dependency.sh"
        return $?
    else
        echo -e "${RED}[!] Error: dependency.sh not found in scripts directory.${NC}"
        exit 1
    fi
}

if [ "$RUN_CHECK_ONLY" = true ]; then
    run_dependency_check
    exit $?
fi

if [ "$AUTO_INSTALL_DEPS" = true ]; then
    source "$SCRIPT_DIR/dependency.sh" > /dev/null
    
    if [ "$HAS_MISSING_CORE" = false ] && [ ${#MISSING_OPT[@]} -eq 0 ]; then
        echo -e "${GREEN}[✓] System environment packages are completely satisfied.${NC}"
        exit 0
    fi

    if [ -f "$SCRIPT_DIR/package-manager.sh" ]; then
        source "$SCRIPT_DIR/package-manager.sh"
        if command -v pacman &> /dev/null; then DISTRO="arch"
        elif command -v apt-get &> /dev/null; then DISTRO="debian"
        elif command -v dnf &> /dev/null; then DISTRO="fedora"
        elif command -v zypper &> /dev/null; then DISTRO="suse"
        fi
        
        echo -e "${BLUE}[*] Dispatching system installations for missing packages...${NC}"
        install_packages "$DISTRO" "${MISSING_CORE[@]}" "${MISSING_OPT[@]}"
    else
        echo -e "${RED}[!] Error: package-manager.sh missing. Cannot run auto-install.${NC}"
        exit 1
    fi
    exit 0
fi

handle_existing_installation() {
    if [ -d "$TARGET_CONFIG_DIR" ]; then
        echo -e "${YELLOW}\n[!] An existing folder was found at ~/.config/hypr-mac."
        echo -e "It may or may not match the configuration version you are trying to install.${NC}"
        echo "What would you like to do?"
        echo "  1. Backup the old folder and do a clean install (Recommended)"
        echo "  2. Keep your current custom files and update the setup scripts safely"
        echo "  3. Cancel the installation entirely (Default)"
        echo -n "Select option [1-3]: "
        read -r user_choice

        case "$user_choice" in
            1)
                HUMAN_TIME=$(date '+%d-%b-%Y_%I-%M%p')
                BACKUP_PATH="${HOME}/.config/hypr-mac_backup_${HUMAN_TIME}"
                echo -e "${BLUE}[*] Safely backing up old files to: $BACKUP_PATH${NC}"
                mv "$TARGET_CONFIG_DIR" "$BACKUP_PATH"
                ;;
            2)
                if [ -f "$SCRIPT_DIR/update.sh" ]; then
                    echo -e "${BLUE}[*] Passing control directly to the update system...${NC}"
                    exec "$SCRIPT_DIR/update.sh" -n
                else
                    echo -e "${RED}[!] Error: update.sh script is missing!${NC}"
                    exit 1
                fi
                ;;
            3|*)
                echo -e "${RED}[!] Installation canceled by user.${NC}"
                exit 0
                ;;
        esac
    fi
}

run_dependency_check
if [ $? -ne 0 ]; then
    echo -e "${RED}[!] Core dependency criteria unmet. Aborting deployment setup.${NC}"
    exit 1
fi

handle_existing_installation

# Dynamic Remote Repository Loader Engine
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
if [ ! -d "$REPO_ROOT/config" ] || [ "$RUN_GIT_CLONE" = true ]; then
    
    # Prompt for permission ONLY if the user didn't explicitly force it with the flag
    if [ "$RUN_GIT_CLONE" = false ]; then
        echo -e "${YELLOW}[!] Core configuration files are missing locally."
        echo -n "Would you like to download a fresh copy of the repository from GitHub? [y/N]: "${NC}
        read -r clone_confirm
        clone_confirm_lower=$(echo "$clone_confirm" | tr '[:upper:]' '[:lower:]')
        
        if [[ "$clone_confirm_lower" != "y" && "$clone_confirm_lower" != "yes" ]]; then
            echo -e "${RED}[!] Installation aborted. Cannot proceed without configuration files.${NC}"
            exit 1
        fi
    fi

    if ! command -v git &> /dev/null; then
        echo -e "${RED}[!] Error: git is required to clone the repository. Run with -i first.${NC}"
        exit 1
    fi
    
    TEMP_CLONE_DIR=$(mktemp -d)
    echo -e "${BLUE}[*] Cloning a fresh configuration manifest from GitHub...${NC}"
    if git clone --depth 1 "$REPO_URL" "$TEMP_CLONE_DIR"; then
        REPO_ROOT="$TEMP_CLONE_DIR"
        echo -e "${GREEN}[✓] Remote repository cloned successfully.${NC}"
    else
        echo -e "${RED}[!] Error: Failed to contact GitHub repository server.${NC}"
        exit 1
    fi
fi

echo -e "${BLUE}[*] Creating configuration layout directory paths...${NC}"
mkdir -p "$TARGET_CONFIG_DIR"

echo "# Hyprland-Mac Deployment Tracking State Manifest" > "$MANIFEST_FILE"
echo "VERSION=1.0.0" >> "$MANIFEST_FILE"
echo "INSTALL_DATE=$(date '+%Y-%m-%d %H:%M:%S')" >> "$MANIFEST_FILE"
echo "DEPLOYED_FILES:" >> "$MANIFEST_FILE"

# The Content Extraction Engine
echo -e "${BLUE}[*] Extracting internal configuration templates into place...${NC}"
if [ -d "$REPO_ROOT/config" ]; then
    cp -r "$REPO_ROOT/config/." "$TARGET_CONFIG_DIR/"
    for source_item in "$REPO_ROOT"/config/*; do
        [ -e "$source_item" ] || continue
        item_name=$(basename "$source_item")
        echo "$TARGET_CONFIG_DIR/$item_name" >> "$MANIFEST_FILE"
        echo -e "  [Extracted Element]   $item_name -> $TARGET_CONFIG_DIR/$item_name"
    done
else
    echo -e "${RED}[!] Error: Source configuration directory 'config/' not found.${NC}"
    exit 1
fi

if [ "$SKIP_DESKTOP" = false ]; then
    echo -e "${BLUE}[*] Building Wayland Desktop Session Entry...${NC}"
    TEMP_DESKTOP=$(mktemp)
    cat <<EOF > "$TEMP_DESKTOP"
[Desktop Entry]
Name=Hyprland (Mac)
Comment=An intelligent dynamic tiling Wayland compositor with Mac layout
Exec=start-hyprland -- --config \$HOME/.config/hypr-mac/hypr/hyprland.lua
DesktopNames=Hardcoded
Type=Application
EOF

    echo -e "${YELLOW}[sudo] Moving session entry file to system environment paths:${NC}"
    sudo mv "$TEMP_DESKTOP" "$DESKTOP_ENTRY_PATH"
    sudo chmod 644 "$DESKTOP_ENTRY_PATH"
    echo "$DESKTOP_ENTRY_PATH" >> "$MANIFEST_FILE"
fi

echo -e "\n${GREEN}[✓] Hyprland-Mac successfully deployed! Session manifest created.${NC}"
