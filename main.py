from pathlib import Path
import platform
import os
from datetime import datetime
import shutil
import configparser
from time import sleep
import colorama
import subprocess
# import sys

MINECRAFT_VERSION = "1.21.5"

def install_fabric(path):
    """
    Install Fabric mod loader for Minecraft.
    This function should handle downloading and installing Fabric.
    """
    # Load config to check if fabric installation is enabled
    config = configparser.ConfigParser()
    config.read("config.ini")

    # Check if fabric installation is enabled in config
    if config.has_section("extras") and config.getboolean("extras", "install_fabric", fallback=False):
        print(colorama.Fore.YELLOW + "[+] Fabric installation enabled in config")
        
        # Look for fabric installer jar in assets folder
        assets_folder = Path("assets")
        fabric_jar = None
        
        if assets_folder.exists():
            # Find fabric installer jar file
            fabric_jars = list(assets_folder.glob("fabric-installer*.jar"))
            if fabric_jars:
                fabric_jar = fabric_jars[0]  # Use first found
            else:
                print(colorama.Fore.RED + "[!] No Fabric installer jar found in assets folder")
                return
        else:
            print(colorama.Fore.RED + "[!] Assets folder not found")
            return
        
        try:
            # Execute fabric installer jar
            command = ["java", "-jar", str(fabric_jar), "client", "-dir", str(path), "-mcversion", MINECRAFT_VERSION]
            print(colorama.Fore.CYAN + f"[+] Executing command: {' '.join(command)}")
            
            result = subprocess.run(command, capture_output=True, text=True, check=True)
            
            if result.stderr:
                print(colorama.Fore.YELLOW + f"[+] Errors: {result.stderr}")
            
            print(colorama.Fore.GREEN + "[✓] Fabric installer executed successfully")
        except subprocess.CalledProcessError as e:
            print(colorama.Fore.RED + f"[!] Fabric installer failed: {e}")
            print("" + colorama.Fore.YELLOW + f"Are you sure minecraft is actually installed in the destination folder: {path}?\n")
            return False
        except FileNotFoundError:
            print(colorama.Fore.RED + "[!] Java not found. Please install Java to run Fabric installer")
        except Exception as e:
            print(colorama.Fore.RED + f"[!] Error running Fabric installer: {e}")
    else:
        print(colorama.Fore.YELLOW + "[+] Fabric installation disabled in config")

def detect_minecraft_path():
    """
    Auto-detect Minecraft installation path based on OS.
    Allow override via config.
    
    Returns: the path to the Minecraft directory.
    Raises:
        OSError if unsupported OS.
        FileNotFoundError if Minecraft directory does not exist.
        NotADirectoryError if Minecraft path is not a directory.
        PermissionError if Minecraft directory is not readable.
        ValueError if Minecraft path is not specified in config.
        Exception for any other errors.
    """
    
    # Load config to check for custom minecraft path
    config = configparser.ConfigParser()
    config.read("config.ini")
    
    # Check if custom minecraft path is specified in config
    if config.has_section("paths") and config.has_option("paths", "minecraft_path"):
        custom_path = config.get("paths", "minecraft_path")
        if custom_path:
            return Path(custom_path)
    
    # Auto-detect based on OS if no custom path specified
    system = platform.system()
    
    if system == "Windows":
        # Windows: %APPDATA%\.minecraft
        return Path(os.path.expandvars("%APPDATA%")) / ".minecraft"
    elif system == "Darwin":  # macOS
        # macOS: ~/Library/Application Support/minecraft
        return Path.home() / "Library" / "Application Support" / "minecraft"
    elif system == "Linux":
        # Linux: ~/.minecraft
        return Path.home() / ".minecraft"
    else:
        raise OSError(f"Unsupported operating system: {system}")

def load_config(config_path="config.ini"):
    """
    Load configuration from file.
    Returns: a dictionary with config values.
    """

    config = configparser.ConfigParser()
    config.read(config_path)

    # Default values
    config_data = {
        "source_base": None,
        "sync_mods": True,
        "sync_resourcepacks": True,
        "sync_shaderpacks": True,
    }

    # Parse [paths] section
    if config.has_section("paths"):
        config_data["source_base"] = config.get("paths", "source_base", fallback=None)
        config_data["minecraft_path"] = config.get("paths", "minecraft_path", fallback=None)

    # Parse [folders] section
    if config.has_section("folders"):
        config_data["sync_mods"] = config.getboolean("folders", "sync_mods", fallback=True)
        config_data["sync_resourcepacks"] = config.getboolean("folders", "sync_resourcepacks", fallback=True)
        config_data["sync_shaderpacks"] = config.getboolean("folders", "sync_shaderpacks", fallback=True)

    return config_data

def backup_folder(target_folder: Path, retention_policy: int = 3):
    """
    Rename original folder (mods, resourcepacks, shaderpacks) by appending .bak.
    Maintain backup retention policy.
    """
    if not target_folder.exists():
        return
    
    # Create backup with timestamp
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    backup_name = f"{target_folder.name}.bak.{timestamp}"
    backup_path = target_folder.parent / backup_name
    
    # Rename current folder to backup
    target_folder.rename(backup_path)
    
    # Clean up old backups based on retention policy
    backup_pattern = f"{target_folder.name}.bak.*"
    existing_backups = sorted(
        target_folder.parent.glob(backup_pattern),
        key=lambda p: p.stat().st_mtime,
        reverse=True
    )
    
    print(colorama.Fore.BLUE + f"[+] Backup created: {backup_path}")

    # Remove excess backups
    for backup in existing_backups[retention_policy:]:
        shutil.rmtree(backup, ignore_errors=True)

def sync_folder(src: Path, dst: Path):
    """
    Copy files from source to destination.
    Handle file conflicts and overwrites.
    """
    if not src.exists():
        print(f"Source folder does not exist: {src}")
        return False
    
    # Create destination folder if it doesn't exist
    dst.mkdir(parents=True, exist_ok=True)
    
    try:
        # Copy all files and subdirectories from source to destination
        for item in src.rglob("*"):
            if item.is_file():
                # Calculate relative path from source
                relative_path = item.relative_to(src)
                dest_file = dst / relative_path
                
                # Create parent directories if needed
                dest_file.parent.mkdir(parents=True, exist_ok=True)
                
                # Copy file, overwriting if exists
                shutil.copy2(item, dest_file)
                
        print(colorama.Fore.BLUE + f"[✓] Successfully synced {src} to {dst}")
        return True
        
    except Exception as e:
        print(f"Error syncing {src} to {dst}: {e}")
        return False

def validate_paths(paths: dict):
    """
    Verify source and target paths exist.
    """
    if not paths.get("minecraft_path"):
        raise ValueError("Minecraft path not specified")
    
    minecraft_path = Path(paths["minecraft_path"])
    if not minecraft_path.exists():
        raise FileNotFoundError(f"Minecraft directory does not exist: {minecraft_path}")
    if not minecraft_path.is_dir():
        raise NotADirectoryError(f"Minecraft path is not a directory: {minecraft_path}")
    if not os.access(minecraft_path, os.R_OK):
        raise PermissionError(f"No read permission for Minecraft directory: {minecraft_path}")
    
    if not paths.get("source_base"):
        raise ValueError("Source base path not specified")
    
    source_base = Path(paths["source_base"])
    if not source_base.exists():
        raise FileNotFoundError(f"Source base directory does not exist: {source_base}")
    
    if not source_base.is_dir():
        raise NotADirectoryError(f"Source base path is not a directory: {source_base}")
    
    # Check if minecraft path is readable
    if not os.access(minecraft_path, os.R_OK):
        raise PermissionError(f"No read permission for Minecraft directory: {minecraft_path}")
    
    # Check if source path is readable
    if not os.access(source_base, os.R_OK):
        raise PermissionError(f"No read permission for source directory: {source_base}")
    
    return True

def check_disk_space(path: Path):
    """
    Check available disk space before syncing.
    """
    pass  # TODO: Implement disk space check

def validate_file_integrity(src: Path, dst: Path):
    """
    Validate file integrity after copy.
    """
    pass  # TODO: Implement integrity check

def rollback():
    """
    Rollback changes in case of failure.
    """
    try:
        minecraft_path = detect_minecraft_path()
        
        # Find the most recent backup for each folder type
        folder_types = ["mods", "resourcepacks", "shaderpacks"]
        
        for folder_type in folder_types:
            current_folder = minecraft_path / folder_type
            backup_pattern = f"{folder_type}.bak.*"
            
            # Find all backups for this folder type
            existing_backups = sorted(
                minecraft_path.glob(backup_pattern),
                key=lambda p: p.stat().st_mtime,
                reverse=True
            )
            
            if existing_backups:
                # Get the most recent backup
                latest_backup = existing_backups[0]
                
                # Remove current folder if it exists
                if current_folder.exists():
                    shutil.rmtree(current_folder, ignore_errors=True)
                
                # Restore from backup
                latest_backup.rename(current_folder)
                print(f"Restored {folder_type} from backup: {latest_backup.name}")
            else:
                print(f"No backup found for {folder_type}")
        
        print("Rollback completed successfully")
        return True
        
    except Exception as e:
        print(f"Error during rollback: {e}")
        return False

def parse_cli_args():
    """
    Parse command-line arguments for one-time operations or interactive mode.
    """
    pass  # TODO: Implement CLI parsing

# def resource_path(relative_path):
#     """ Get absolute path to resource, works for dev and for PyInstaller .exe """
#     try:
#         base_path = sys._MEIPASS  # PyInstaller sets this attr
#     except AttributeError:
#         base_path = os.path.abspath(".")
#     return os.path.join(base_path, relative_path)

def main():
    os.system('cls' if os.name == 'nt' else 'clear')  # Clear console for better readability
    colorama.init(autoreset=True)
    print(colorama.Fore.GREEN + f"Minecraft {MINECRAFT_VERSION} Sync Tool")
    print(colorama.Fore.GREEN + "==========================")
    sleep(0.1)

    try:
        
        # Load config
        config = load_config()

        # Parse CLI arguments
        # TODO: Implement CLI parsing when needed
        
        # Detect Minecraft path
        minecraft_path = detect_minecraft_path()


        # Use current working directory if source_base is not set
        source_base = Path(config["source_base"]) if config["source_base"] else Path.cwd()
        paths = {
            "minecraft_path": minecraft_path,
            "source_base": source_base
        }
        

        print(colorama.Fore.YELLOW + f"Using Source Folder: {source_base}")
        print(colorama.Fore.YELLOW + f"Using Minecraft path: {minecraft_path}")
        sleep(0.2)
        print()

        # install fabric if enabled in config
        fabric_result = install_fabric(minecraft_path)
        if fabric_result is False:
            print(colorama.Fore.RED + "[!] Fabric installation failed. Please cross check config.ini and ensure Minecraft is installed correctly.")
            return

        # Validate paths and disk space
        validate_paths(paths)
        sleep(0.2)
        print()
        
        # Backup original folders and sync
        folder_types = ["mods", "resourcepacks", "shaderpacks"]
        config_keys = ["sync_mods", "sync_resourcepacks", "sync_shaderpacks"]
        
        sync_results = []
        
        for folder_type, config_key in zip(folder_types, config_keys):
            if not config[config_key]:
                print(f"Skipping {folder_type} (disabled in config)")
                continue
                
            src_folder = source_base / folder_type
            dst_folder = minecraft_path / folder_type
            
            # Backup original folder
            backup_folder(dst_folder)
            
            # Sync folders
            success = sync_folder(src_folder, dst_folder)
            sync_results.append((folder_type, success))
            print()
            sleep(0.4)

        # Validate integrity
        # TODO: Implement integrity validation
        
        # Print sync report
        print(colorama.Fore.GREEN + "\n=== Sync Report ===")
        for folder_type, success in sync_results:
            status = "✓ SUCCESS" if success else "✗ FAILED"
            color = colorama.Fore.GREEN if success else colorama.Fore.RED
            print(color + f"{folder_type}: {status}")
            sleep(0.2)
        
        if all(result[1] for result in sync_results):
            print(colorama.Fore.GREEN + "Packs have been installed successfully!")
            sleep(0.2)
        else:
            print(colorama.Fore.RED + "Some folders failed to sync. Check the output above for details.")
            sleep(0.3)
            
        
        print("\nPress Enter to exit...")
        input()
    except Exception as e:
        print(f"Error during sync: {e}")
        print("Attempting rollback...")
        rollback()

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print(colorama.Fore.RED + "\nProcess interrupted by user. Exiting...")
