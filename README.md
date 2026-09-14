# Hyprland-mac

This repository contains pre-built configurations for lightweight external tools, designed to mimic the aesthetics of macOS Tahoe 26.6. 

If you encounter any issues or have feedback, please open an issue and I will try my best to resolve it.

### Requirements 
1. **Linux Distro**: Any distribution that supports Hyprland (Wayland).
2. **Hyprland**: Version ≥ 0.55 (required for Lua script support).
3. **awww**: For wallpaper management.
4. **nwg-dock-hyprland**: For the macOS-style dock.
5. **Waybar**: For the top bar and toolbar. **[See Upstream Dependency Note Below]**
6. **mako**: For notification management.
7. **Rofi** *(Optional)*: For the application launcher/Launchpad.
8. **Spotlight Search** *(Optional)*: Global desktop lookup framework. **[Currently Under Development]**
9. **Waypaper** *(Optional)*: A graphical frontend for instant wallpaper selection.
10. **wpp-cli** *(Optional)*: A custom, high-speed terminal engine built for advanced wallpaper manipulation.
11. **Apple Fonts** *(Optional)*: San Francisco or similar TTF fonts for the best text experience.
12. **grim & slurp** *(Highly Recommended)*: For taking screenshots.

> [!WARNING]
> **Upstream Dependency Note:** Due to recent breaking API changes in Hyprland v0.55+ regarding Lua integration, the standard stable release of Waybar currently experiences an upstream bug affecting workspace click-dispatchers. Please ensure you are utilizing `waybar-git` (AUR) to maintain full workspace interactivity.

## Script Automation Status (Fixed)

The automated helper deployment script suite is completely fixed and operational. Manual directory copy operations have been fully automated. The engine handles cross-distro dependency validation, smart repository extraction, updates, and manifest tracking out-of-the-box.

> [!IMPORTANT]
> Do **not** run the installation scripts with `sudo` privileges (root). The script will prompt you for password access internally only when necessary (e.g., updating packages or creating session entry objects).

### Installation Instructions

1. Clone the repository:
   ```bash
   git clone https://github.com/InkTeflix/Hyprland-mac.com
   ```
2. Navigate into the script toolkit directory:
   ```bash
   cd Hyprland-mac/scripts
   ```
3. Run the automated interactive installer wizard directly:
   ```bash
   ./install.sh
   ```

### Script Toolkit Control Flags

* **`-c, --check`**: Runs dependency verification tests safely without making system changes.
* **`-i, --install-deps`**: Automatically queries and fetches missing software packages via your native package manager.
* **`-n, --no-desktop`**: Deploys layout profiles but skips generating the Wayland login session entry.
* **`-g, --git-pull`**: Forces the tool to download a fresh clone directly from the remote GitHub server.

---

## The wpp-cli Engine (Optional Native Tool)

Once `install.sh` deploys your environment, a raw Python utility script is dropped right into `~/.config/hypr-mac/wallpaper`. This serves as an ultra-lightweight terminal alternative to heavy graphic user interfaces like `Waypaper`. It is built purely for speed, background resource efficiency, and advanced execution modes.

### Automated Daytime & Wallpaper Family Management
All default repository assets utilize a structured naming scheme where the text preceding the first dash (`-`) or underscore (`_`) defines a specific **wallpaper family**. The script reads these tokens to execute smart, automated day-to-night transitions.

### 4 Distinct Operation Modes
Running the CLI utility opens up access to advanced automation behaviors:
* **`family-time`**: Transitions assets chronologically based on matching family tags.
* **`rotate-time`**: Steps through different background styles over custom time intervals.
* **`periodic`**: Refreshes environments on explicit, fixed cycles.
* **`manual`**: Gives you immediate, direct terminal command execution control over backgrounds.
* *Query active states, explore internal commands, and check options instantly with the `-h / --help` flag.*

### Clean Shell Aliasing Engine (Zero Intrusive Footprint)
To prevent typing long system folder paths every single time you want to switch frames, you can execute:
```bash
~/.config/hypr-mac/wallpaper setup
```
This utility automatically reads your current shell environment configuration and displays the precise directions needed to link the short **`wpp`** terminal command. 

For strict user convenience, **the script makes absolutely zero modifications outside the hypr-mac directory.** It simply generates a single text string line for you to copy and paste directly into your local `~/.bashrc` or `~/.zshrc` file. This ensures you keep total, manual control over your shell configuration files and can wipe the line whenever you see fit. 

If the shell integration setup is completed, the system unlocks tab-based shell auto-completion for all wallpaper asset filenames and mode parameters!
