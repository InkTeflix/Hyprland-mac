# Hyprland-mac

### requirements 
1. *linux* -any linux distro that supports hyprland (wayland).
2. *Hyprland -verion 5.55+* -for lua scrpit.
3. *Waybar* -for the top bar.
4. *Rofi* -for the app launcher/launchpad.
5. *Awww(swww)* -for wallpaper.
6. *nwg-dock-hyprland* -for dock.
7. *apple-fonts* -for ttf (usually, things will go smoothly even with).

## Extra changes

This configurations(beta) are being provided considering that the user may want try it, and are thus made using different directories then the default ones. Anything, will not make any permanent changes to the system.
> [!TIP]
> To completely remove the configurations from system, just remove the ~/.config/hypr-mac directory (if it was created) and/or the custom desktop entry at /use/share/wayland-sessions/hyprland-mac.desktop (if it was created).

> [!WARNING]
> It is highly _unrecommened_ to run the script with sudo privilege (root).
> Only give the access at the time of desktop file creation.

<details>
<summary><b>Safety precautions for testing purpose</b></summary>
It is recommended, that you create a new desktop session entry. That way you can test this,or any other dot file,without messing with your current configurations (if you have any, that you love).

</details>

> [!NOTE]
> It is recommended to do the system level changes carefully, if doing manually.

<details>
<summary><b>System level changes</b></summary>

Here are the step-by-step instructions:
1. Clone the repository.
2. Run the install.sh. OR if desired to do manually, you can create the .desktop entry in /use/share/wayland-sessions/hyprland-mac.desktop

</details>
