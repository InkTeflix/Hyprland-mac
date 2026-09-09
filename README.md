# Hyprland-mac

This repository contains pre-built configurations for lightweight external tools, designed to mimic the aesthetics of macOS Tahoe 26.6. 

If you encounter any issues or have feedback, please open an issue and I will try my best to resolve it.

### Requirements 
1. **Linux Distro**: Any distribution that supports Hyprland (Wayland).
2. **Hyprland**: Version ≥ 0.55 (required for Lua script support).
3. **awww**: For wallpaper management.
4. **nwg-dock-hyprland**: For the macOS-style dock.
5. **Waybar**: For the top bar and toolbar (Global Menu support is currently under development)(current stable version broke the change focused workspace on-click, solution? waybar-git).
6. **mako**: For notification management.
7. **Rofi** *(Optional)*: For the application launcher/Launchpad.
8. **Waypaper** *(Optional)*: A GUI frontend for instant wallpaper selection.
9. **Apple Fonts** *(Optional)*: San Francisco or similar TTF fonts for the best text experience.
10. **grim & slurp** *(Highly Recommended)*: For taking screenshots.

## Configuration Details & Safety

These beta configurations use separate directories from your default system paths. Testing this setup will not make permanent or destructive changes to your existing environment.

> [!TIP]
> To completely remove the configurations from system, just remove the ~/.config/hypr-mac directory (if it was created) and/or the custom desktop entry at /use/share/wayland-sessions/hyprland-mac.desktop (if it was created).
> To completely remove this configuration, simply delete the `~/.config/hypr-mac` directory and the custom desktop entry located at `/usr/share/wayland-sessions/hyprland-mac.desktop` (if they were created).

> [!WARNING]
> It is highly _unrecommened_ to run the script with sudo privilege (root).
> Only give the access at the time of desktop file creation.
> Do **not** run the installation script with `sudo` privileges (root). The script will prompt you for password access only when necessary (e.g., updating packages or creating the `.desktop` file).

<details>
<summary><b>Safety precautions for testing purpose</b></summary>
It is recommended, that you create a new desktop session entry. That way you can test this,or any other dot file,without messing with your current configurations (if you have any, that you love).
<summary><b>Safety Precautions for Testing</b></summary>

I highly recommend creating a new desktop session entry. This allows you to safely test these dotfiles without interfering with your current desktop environment configuration.
</details>

> [!NOTE]
> It is recommended to do the system level changes carefully, if doing manually.
> Please be careful when applying system-level changes manually.

<details>
<summary><b>System level changes</b></summary>

Here are the step-by-step instructions:
1. Clone the repository. 
2. Change the directory to the copied path of repo.
3. Run the install.sh (currently removed cuz of bugs :). OR, if desired to do manually, you can create the .desktop entry in /use/share/wayland-sessions/hyprland-mac.desktop.
<summary><b>Manual Installation Steps</b></summary>

1. Clone the repository:
   ```bash
   git clone https://github.com/InkTeflix/Hyprland-mac.git
   ```
2. Navigate into the repository directory:
   ```bash
   cd hyprland-mac
   ```
3. Run the installer script (Note: `install.sh` is temporarily removed due to bugs). Alternatively, you can manually create the desktop entry at `/usr/share/wayland-sessions/hyprland-mac.desktop`.
   ```ini
   [Desktop Entry]
   Name=Hyprland (Mac)
   Comment=An intelligent dynamic tiling Wayland compositor with Mac layout
   Exec=start-hyprland -- --config $HOME/.config/hypr-mac/hypr/hyprland.lua
   DesktopNames=Hyprland
   Type=Application
   ```

</details>

> [!NOTE]
> Please change the directory if the cloned location differs, when creating manually.
