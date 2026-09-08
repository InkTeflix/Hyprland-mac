# Hyprland-mac

These are pre-build configuration, that can be used on some of the least resource heavy *external* tools for the best *aesthetics* and looks inspired by MacOS Tahoe 26.6.
Any *and* all complain will be *tried* to be resolved.

### requirements 
1. *linux* -any linux distro that supports hyprland (wayland).
2. *Hyprland -verion 0.55+* -for hyprland.lua support.
3. *Awww (swww)* -for wallpaper.
4. *nwg-dock-hyprland* -for dock.
5. *Waybar* -for the top bar & tool bar (optional, mainly for global menu).
6. *Rofi* -for the app launcher/launchpad (optional, makes opening apps easier, mainly for looks).
7. *waypaper* -for instant selection and application of wallpaper (completely optional, helps changing wallpaper easier, doesn't affect stability).
8. *apple-fonts* -for ttf (completely optional)(usually, things will go smoothly even with, unless you want best text~feel).
9. *eww* -for desktop widgets (completely optional, unless you usually spend a lot of time on empty desktop).
10. *grim* & *slurp* -for screen shots (completely optional, but highly recommanded)

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
2. Change the directory to the copied path of repo.
3. Run the install.sh (currently removed cuz of bugs :). OR, if desired to do manually, you can create the .desktop entry in /use/share/wayland-sessions/hyprland-mac.desktop.

</details>
