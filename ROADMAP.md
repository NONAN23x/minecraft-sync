# Minecraft Sync Program Plan

## Overview
A cross-platform utility to synchronize Minecraft mods, resource packs, and shader packs across different client installations with automatic backup functionality.

## Core Features
- **Cross-platform compatibility** (Windows, macOS, Linux)
- **Automatic backup** of existing files before sync
- **Multi-folder sync** support (mods, resourcepacks, shaderpacks)
- **Configuration-based** source and destination mapping

## Technical Requirements

### Language & Framework
- **Python 3.8+** for cross-platform compatibility
- **pathlib** for path handling
- **shutil** for file operations
- **configparser** for configuration management

### Directory Structure
```
minecraft-sync/
├── sync.py
├── config.ini
├── README.md
├── mods/
│   └── README.md (contains links and credits to authors)
├── resourcepacks/
│   └── README.md (contains links and credits to authors)
└── shaderpacks/
    └── README.md (contains links and credits to authors)
```

## Implementation Plan

### Phase 1: Core Functionality
1. **Path Detection**
    - Auto-detect Minecraft installation paths per OS
    - Support custom path configuration

2. **Backup System**
    - Rename the original folders (mods, resourcepacks, shaderpacks by appending .bak to folder name)
    - Maintain backup retention policy

3. **Sync Engine**
    - Copy files from source (project repository folders, mods, resourcepacks and shaderpacks) to destination 
    - Handle file conflicts and overwrites

### Phase 2: Configuration
1. **Config File Format**
    ```ini
    [paths]
    target_base = /path/to/target/minecraft # scale the program for some users using legacy launcher with custom game folder location
    
    [folders]
    sync_mods = true
    sync_resourcepacks = true
    sync_shaderpacks = true
    ```

2. **CLI Interface**
    - Command-line arguments for one-time operations
    - Interactive mode for configuration

### Phase 3: Safety Features
1. **Validation**
    - Verify source and target paths exist
    - Check available disk space
    - Validate file integrity

2. **Error Handling**
    - Graceful failure recovery
    - Detailed logging
    - Rollback capability

## Usage Workflow
1. Run script with `uv run sync.py`
2. System detects Minecraft paths
3. Creates backup of target folders
4. Copies source files to target locations
5. Generates sync report

## Platform-Specific Considerations

### Windows
- Default path: `%APPDATA%\.minecraft`
- Handle long path names
- Windows file permissions

### macOS
- Default path: `~/Library/Application Support/minecraft`
- Handle case-sensitive filesystem

### Linux
- Default path: `~/.minecraft`
- Handle various distributions
- Permission management

## Future Enhancements
- GUI interface
- Selective sync (individual mods/packs)
- Cloud storage integration
- Version control for mods