#!/usr/bin/env bash

# Define minimum version tokens for upstream safety
export REQ_HYPRLAND_VER="0.55"

show_help() {
    echo -e "\033[0;34m=== Hyprland-Mac Package Manager Abstraction Layer ===\033[0m"
    echo "Purpose: Provides a backend library of package distribution maps, native query strings,"
    echo "         and unified cross-distribution software installation routines."
    echo ""
    echo "Usage: This script is a library file meant to be sourced into your main tools."
    echo "       Direct usage is limited to reading database mappings."
    echo ""
    echo "Options:"
    echo "  -h, --help    Show this package manager library documentation."
    exit 0
}

# Parse parameters passed directly to the library file
if [[ "$1" == "-h" || "$1" == "--help" ]]; then
    show_help
fi

# --- Distro Map Resolution ---
# Maps internal abstract names to exact package strings per distro
get_package_name() {
    local distro="$1"
    local abstract_name="$2"

    case "$distro" in
        "arch")
            case "$abstract_name" in
                "hyprland")          echo "hyprland" ;;
                "awww")              echo "awww" ;;
                "nwg-dock-hyprland") echo "nwg-dock-hyprland" ;;
                "waybar")            echo "waybar-git" ;; # Resolves workspace click bug
                "mako")              echo "mako" ;;
                "git")               echo "git" ;;
                "rofi")              echo "rofi-wayland" ;;
                "waypaper")          echo "waypaper" ;;
                "grim")              echo "grim" ;;
                "slurp")             echo "slurp" ;;
            esac
            ;;
        "debian")
            case "$abstract_name" in
                "hyprland")          echo "hyprland" ;;
                "awww")              echo "awww" ;;
                "nwg-dock-hyprland") echo "nwg-dock-hyprland" ;;
                "waybar")            echo "waybar" ;;
                "mako")              echo "mako" ;;
                "git")               echo "git" ;;
                "rofi")              echo "rofi" ;;
                "waypaper")          echo "waypaper" ;;
                "grim")              echo "grim" ;;
                "slurp")             echo "slurp" ;;
            esac
            ;;
        "fedora")
            case "$abstract_name" in
                "hyprland")          echo "hyprland" ;;
                "awww")              echo "awww" ;;
                "nwg-dock-hyprland") echo "nwg-dock-hyprland" ;;
                "waybar")            echo "waybar" ;;
                "mako")              echo "mako" ;;
                "git")               echo "git" ;;
                "rofi")              echo "rofi" ;;
                "waypaper")          echo "waypaper" ;;
                "grim")              echo "grim" ;;
                "slurp")             echo "slurp" ;;
            esac
            ;;
        "suse")
            case "$abstract_name" in
                "hyprland")          echo "hyprland" ;;
                "awww")              echo "awww" ;;
                "nwg-dock-hyprland") echo "nwg-dock-hyprland" ;;
                "waybar")            echo "waybar" ;;
                "mako")              echo "mako" ;;
                "git")               echo "git" ;;
                "rofi")              echo "rofi" ;;
                "waypaper")          echo "waypaper" ;;
                "grim")              echo "grim" ;;
                "slurp")             echo "slurp" ;;
            esac
            ;;
    esac
}

# --- Native Version Query Engine ---
# Queries the package manager db directly to see if the package is registered
is_package_installed() {
    local distro="$1"
    local pkg="$2"

    case "$distro" in
        "arch")
            pacman -Qq "$pkg" &> /dev/null && return 0
            ;;
        "debian")
            dpkg-query -W -f='${Status}' "$pkg" 2>&1 | grep -q "ok installed" && return 0
            ;;
        "fedora"|"suse")
            rpm -q "$pkg" &> /dev/null && return 0
            ;;
    esac
    return 1
}

# --- Automated Installation Dispatcher ---
# Executed safely via: ./install.sh -i
install_packages() {
    local distro="$1"
    shift
    local pkgs=("${@}")

    [ ${#pkgs[@]} -eq 0 ] && return 0

    echo -e "\nRunning native package manager installer routines..."
    case "$distro" in
        "arch")
            # Uses yay or paru if available for AUR dependencies (nwg-dock, waybar-git)
            if command -v yay &> /dev/null; then
                yay -S --noconfirm "${pkgs[@]}"
            elif command -v paru &> /dev/null; then
                paru -S --noconfirm "${pkgs[@]}"
            else
                sudo pacman -S --noconfirm "${pkgs[@]}"
            fi
            ;;
        "debian")
            sudo apt-get update && sudo apt-get install -y "${pkgs[@]}"
            ;;
        "fedora")
            sudo dnf install -y "${pkgs[@]}"
            ;;
        "suse")
            sudo zypper install -y "${pkgs[@]}"
            ;;
    esac
}
