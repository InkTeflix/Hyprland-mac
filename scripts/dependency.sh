#!/usr/bin/env bash

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

show_help() {
    echo -e "${BLUE}=== Hyprland-Mac Dependency Checker ===${NC}"
    echo "Purpose: Evaluates system binaries, packages, and software version steps"
    echo "         to guarantee cross-distribution runtime environment stability."
    echo ""
    echo "Usage: ./dependency.sh [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  -h, --help     Show this dependency verification document window."
    echo "  -q, --quiet    Execute checks silently (relies strictly on system exit codes)."
    exit 0
}

QUIET_MODE=false
while [[ "$#" -gt 0 ]]; do
    case "$1" in
        -h|--help) show_help ;;
        -q|--quiet) QUIET_MODE=true; shift ;;
        *) echo -e "${RED}Unknown parameter: $1${NC}"; show_help; exit 1 ;;
    esac
done

SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"
if [ -f "$SCRIPT_DIR/package-manager.sh" ]; then
    source "$SCRIPT_DIR/package-manager.sh"
else
    [ "$QUIET_MODE" = false ] && echo -e "${RED}[!] Critical: package-manager.sh missing!${NC}"
    exit 1
fi

detect_distro() {
    if command -v pacman &> /dev/null; then DISTRO="arch"
    elif command -v apt-get &> /dev/null; then DISTRO="debian"
    elif command -v dnf &> /dev/null; then DISTRO="fedora"
    elif command -v zypper &> /dev/null; then DISTRO="suse"
    else
        [ "$QUIET_MODE" = false ] && echo -e "${RED}[!] Unsupported distribution.${NC}"
        exit 1
    fi
}
detect_distro

[ "$QUIET_MODE" = false ] && echo -e "System Configuration: ${GREEN}${DISTRO^^}${NC}\n"

export MISSING_CORE=()
export MISSING_OPT=()

verify_binary_version() {
    local binary="$1"
    if [ "$binary" = "hyprland" ]; then
        if command -v hyprland &> /dev/null; then
            local current_ver
            current_ver=$(hyprland --version | head -n 1 | awk '{print $3}' | sed 's/v//' | cut -d'.' -f1-2)
            if [ "$(echo -e "$current_ver\n$REQ_HYPRLAND_VER" | sort -V | head -n1)" != "$REQ_HYPRLAND_VER" ]; then
                echo -e " (Outdated: Found $current_ver, Needs >= $REQ_HYPRLAND_VER)"
                return 1
            fi
        fi
    fi
    return 0
}

ABSTRACT_CORE=("hyprland" "awww" "nwg-dock-hyprland" "waybar" "mako" "git")
ABSTRACT_OPT=("rofi" "waypaper" "grim" "slurp")
HAS_MISSING_CORE=false

check_dependencies() {
    local type="$1"
    shift
    local targets=("${@}")

    [ "$QUIET_MODE" = false ] && echo -e "${BLUE}Scanning ${type} Components...${NC}"
    for item in "${targets[@]}"; do
        local native_pkg
        native_pkg=$(get_package_name "$DISTRO" "$item")

        if command -v "$item" &> /dev/null || is_package_installed "$DISTRO" "$native_pkg"; then
            if ! version_err=$(verify_binary_version "$item"); then
                [ "$QUIET_MODE" = false ] && echo -e "  [${RED}✗${NC}] $native_pkg$version_err"
                if [ "$type" = "Core" ]; then
                    MISSING_CORE+=("$native_pkg")
                    HAS_MISSING_CORE=true
                else
                    MISSING_OPT+=("$native_pkg")
                fi
            else
                [ "$QUIET_MODE" = false ] && echo -e "  [${GREEN}✓${NC}] $native_pkg (Verified)"
            fi
        else
            [ "$QUIET_MODE" = false ] && echo -e "  [${RED}✗${NC}] $native_pkg (Missing)"
            if [ "$type" = "Core" ]; then
                MISSING_CORE+=("$native_pkg")
                HAS_MISSING_CORE=true
            else
                MISSING_OPT+=("$native_pkg")
            fi
        fi
    done
    [ "$QUIET_MODE" = false ] && echo ""
}

check_dependencies "Core" "${ABSTRACT_CORE[@]}"
check_dependencies "Optional" "${ABSTRACT_OPT[@]}"

if [ "$HAS_MISSING_CORE" = true ]; then
    if [ "$QUIET_MODE" = false ]; then
        echo -e "${YELLOW}Outdated or missing required dependencies detected:${NC}"
        echo -e "Missing: ${RED}{ ${MISSING_CORE[*]} }${NC}\n"
        echo -e "Please install/update them manually, or run:"
        echo -e "  ${GREEN}./install.sh -i${NC}"
    fi
    exit 1
else
    if [ "$QUIET_MODE" = false ]; then
        echo -e "${GREEN}[✓] All core dependencies are met!${NC}"
        if [ ${#MISSING_OPT[@]} -ne 0 ]; then
            echo -e "${YELLOW}Note: Some optional packages are missing: { ${MISSING_OPT[*]} }.${NC}"
        fi
    fi
    exit 0
fi
