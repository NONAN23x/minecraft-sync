<div align="center">

# Minecraft Sync 🚀

![Minecraft Version](https://img.shields.io/badge/Minecraft-26.1.2-green?style=for-the-badge&logo=minecraft)
![License](https://img.shields.io/github/license/NONAN23x/minecraft-sync?style=for-the-badge)
![Stars](https://img.shields.io/github/stars/NONAN23x/minecraft-sync?style=for-the-badge)
![Issues](https://img.shields.io/github/issues/NONAN23x/minecraft-sync?style=for-the-badge)


### Supported Platforms 🖥️

![Windows](https://img.shields.io/badge/Windows-0078D6?style=for-the-badge&logo=windows&logoColor=white)
![macOS](https://img.shields.io/badge/macOS-000000?style=for-the-badge&logo=apple&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black)

## Purpose 🎯

Minecraft Sync was born out of frustration with the endless hassle of getting friends' Minecraft setups to match mine! 😤 If you've ever spent hours walking non-tech-savvy friends through mod installations, config tweaks, and version matching just to play together, you know the pain. This tool was originally created for my personal use and my friend group - because life's too short to troubleshoot mod conflicts when you just want to build and explore together! 🎮👥

<br>

<br>

## Prerequisites ⚙️

</div>

Before installing Minecraft-Sync, make sure you have these essentials:

1. **Java 21 or higher** - Download from [Oracle](https://www.oracle.com/java/technologies/downloads/) or [OpenJDK](https://openjdk.org/)
> 💡 **Tip:** Check your Java version by running `java -version` in terminal/command prompt
2. **Minecraft Launcher** - Official launcher from [minecraft.net](https://www.minecraft.net/download) or third-party launchers like [MultiMC](https://multimc.org/) or [Prism Launcher](https://prismlauncher.org/)

<br>

<div align=center>

## Installation 📦

</div>

1. **Install Git** - Download from [git-scm.com](https://git-scm.com/)
2. **Install UV package manager** - Use these quick commands:
    
    - **Windows (PowerShell/Terminal/CommandPrompt):**
        ```powershell
        powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
        ```
        
    - **Linux/macOS:**
        ```bash
        curl -LsSf https://astral.sh/uv/install.sh | sh
        ```
> ⚠️ Notice: Restart your terminal so that git and uv reflect on your PATH

3. **Clone the repository**
    ```bash
    git clone https://github.com/nonan23x/minecraft-sync.git
    cd minecraft-sync
    ```
4. **Install dependencies**
    ```bash
    uv sync
    ```
5. **Run the application**
    ```bash
    uv run main.py
    ```

That's it! You're now ready to enjoy Minecraft like never before! 🎉

> ⚠️ Notice: This program installs the Fabric client and creates a Fabric launcher profile for Minecraft `26.1.2`. Make sure you launch that Fabric profile, not the vanilla one.

<br>

## Understanding the configuration [config.ini] ⚙️
You might want to modify the configuration depending on whether you want to use the default Minecraft path, a custom installation folder, or skip syncing specific folders.

> Change the values to true/false based on requirements
```ini
[paths]
# Leave empty to use the current repository folder as the source
source_base = 

# Leave empty to auto-detect the default Minecraft installation path
# Example: D:\Games\minecraft or /home/yourname/.minecraft
minecraft_path =

[folders]
# Keep them all true by default
sync_mods = true
sync_resourcepacks = true
sync_shaderpacks = true

[extras]
# Reserved for future use; Java installation is not automated yet
install_java = false

# Automatically install Fabric mod loader
# true by default, set to false to skip installation, but you will need to install it manually
install_fabric = true
```

If `source_base` is left empty, the script uses the current working directory as the source for `mods/`, `resourcepacks/`, and `shaderpacks/`.

## Why Use Minecraft Sync? 💡

- 🐌 **Fix Vanilla Performance Issues** - Tired of 30 FPS on a decent PC? Sodium + Lithium combo delivers 3x better performance
- 🎨 **Enhanced Visuals Made Easy** - Pre-configured Iris shaders with BSL, Complementary, Bliss, and Kappa without the usual setup headache
- 📦 **Curated Mod Collection** - Hand-picked QOL mods that actually matter - no bloat, just improvements
- 🖼️ **Faithful Resource Packs** - Consistent visual overhaul that respects Minecraft's original aesthetic
- 🔧 **Skip the Config Hell** - All mods pre-tuned and compatible - no more crashes from conflicting settings
- 👥 **Everyone Stays Updated** - Your friends get the exact same setup automatically, no more "why doesn't my game look like yours?"

Stop fighting with mod incompatibilities and outdated tutorials! 🎮✨

</div>
