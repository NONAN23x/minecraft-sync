use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use owo_colors::OwoColorize;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tempfile::{Builder as TempFileBuilder, TempDir};
use zip::ZipArchive;

const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/NONAN23x/minecraft-sync/releases/latest/download/manifest.json";
const APP_TITLE: &str = "Minecraft Sync Installer";
const MIN_JAVA_VERSION: u32 = 21;
const WINDOWS_JAVA_INSTALLER_URL: &str =
    "https://download.oracle.com/java/26/latest/jdk-26_windows-x64_bin.msi";
const MACOS_JAVA_INSTALLER_URL: &str =
    "https://download.oracle.com/java/26/latest/jdk-26_macos-x64_bin.dmg";

type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    minecraft_version: String,
    release_tag: String,
    assets: Assets,
}

#[derive(Debug, Deserialize)]
struct Assets {
    mods: ReleaseAsset,
    resourcepacks: ReleaseAsset,
    shaderpacks: ReleaseAsset,
    fabric_installer: Option<ReleaseAsset>,
}

#[derive(Debug, Deserialize, Clone)]
struct ReleaseAsset {
    url: String,
    sha256: String,
    size: u64,
}

#[derive(Debug)]
struct Args {
    manifest_url: String,
    minecraft_dir: Option<PathBuf>,
    skip_fabric: bool,
    skip_mods: bool,
    skip_resourcepacks: bool,
    skip_shaderpacks: bool,
    show_help: bool,
}

#[derive(Debug)]
struct FolderState {
    target: PathBuf,
    backup: Option<PathBuf>,
}

#[derive(Debug)]
struct PreflightResult {
    manifest: Manifest,
    minecraft_dir: PathBuf,
    temp_dir: TempDir,
    java_command: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    UserCancelled,
    InvalidMinecraftDir,
    PermissionDenied,
    Network,
    ManifestParse,
    JavaMissing,
    JavaVersionTooOld,
    FabricInstall,
    AssetIntegrity,
    Rollback,
    UnsupportedOs,
    TempDir,
    Other,
}

impl ErrorKind {
    fn is_retryable(self) -> bool {
        matches!(self, Self::Network | Self::ManifestParse)
    }
}

#[derive(Debug)]
struct AppError {
    kind: ErrorKind,
    message: String,
    suggestions: Vec<String>,
    detail: Option<String>,
}

impl AppError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            suggestions: Vec::new(),
            detail: None,
        }
    }

    fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn debug_summary(&self) -> String {
        match &self.detail {
            Some(detail) => format!("{} ({detail})", self.message),
            None => self.message.clone(),
        }
    }
}

#[derive(Debug)]
struct Ui {
    stdin_tty: bool,
    color: bool,
}

impl Ui {
    fn new() -> Self {
        let stdin_tty = io::stdin().is_terminal();
        let stdout_tty = io::stdout().is_terminal();
        let stderr_tty = io::stderr().is_terminal();
        let color = env::var_os("NO_COLOR").is_none() && (stdout_tty || stderr_tty);

        Self { stdin_tty, color }
    }

    fn can_prompt(&self) -> bool {
        self.stdin_tty
    }

    fn banner(&self) {
        println!();
        println!(
            "{}",
            self.paint("========================================", Tone::Banner)
        );
        println!("{}", self.paint(APP_TITLE, Tone::BannerTitle));
        println!(
            "{}",
            self.paint(
                "Checks Java, verifies downloads, and safely syncs your modpack.",
                Tone::BannerText,
            )
        );
        println!(
            "{}",
            self.paint("========================================", Tone::Banner)
        );
        println!();
    }

    fn section(&self, title: &str) {
        println!("{}", self.paint(title, Tone::Section));
    }

    fn step(&self, message: impl AsRef<str>) {
        println!(
            "{} {}",
            self.paint("==>", Tone::StepPrefix),
            self.paint(message.as_ref(), Tone::StepText)
        );
    }

    fn info(&self, message: impl AsRef<str>) {
        println!(
            "{} {}",
            self.paint("[i]", Tone::InfoPrefix),
            message.as_ref()
        );
    }

    fn success(&self, message: impl AsRef<str>) {
        println!(
            "{} {}",
            self.paint("[ok]", Tone::SuccessPrefix),
            message.as_ref()
        );
    }

    fn warn(&self, message: impl AsRef<str>) {
        println!(
            "{} {}",
            self.paint("[!]", Tone::WarnPrefix),
            message.as_ref()
        );
    }

    fn hint(&self, message: impl AsRef<str>) {
        println!(
            "{} {}",
            self.paint("[tip]", Tone::HintPrefix),
            message.as_ref()
        );
    }

    fn error_report(&self, error: &AppError) {
        eprintln!();
        eprintln!(
            "{} {}",
            self.paint("[error]", Tone::ErrorPrefix),
            self.paint("Installation could not continue.", Tone::ErrorTitle)
        );
        eprintln!("{}", error.message);
        for suggestion in &error.suggestions {
            eprintln!("  - {suggestion}");
        }
        if let Some(detail) = &error.detail {
            eprintln!(
                "{}",
                self.paint(&format!("Details: {detail}"), Tone::Detail)
            );
        }
    }

    fn recoverable_error(&self, error: &AppError) {
        self.warn(&error.message);
        for suggestion in &error.suggestions {
            self.hint(suggestion);
        }
        if let Some(detail) = &error.detail {
            self.info(format!("Details: {detail}"));
        }
    }

    fn prompt_line(&self, message: &str) -> AppResult<String> {
        if !self.can_prompt() {
            return Err(AppError::new(
                ErrorKind::Other,
                "Interactive input is not available in this session.",
            )
            .with_suggestion("Run the installer from a terminal if you want guided prompts."));
        }

        print!("{}", self.paint(message, Tone::Prompt));
        io::stdout()
            .flush()
            .map_err(|error| io_error(error, "flush terminal output", None))?;

        let mut input = String::new();
        let bytes_read = io::stdin()
            .read_line(&mut input)
            .map_err(|error| io_error(error, "read your input", None))?;

        if bytes_read == 0 {
            return Err(AppError::new(
                ErrorKind::UserCancelled,
                "The installer stopped because input was closed.",
            ));
        }

        Ok(input)
    }

    fn prompt_retry_or_exit(&self, retry_label: &str) -> AppResult<RetryChoice> {
        loop {
            println!();
            println!("Choose an option:");
            println!("  1) {retry_label}");
            println!("  2) Exit");

            let choice = self.prompt_line("Selection: ")?;
            match choice.trim().to_ascii_lowercase().as_str() {
                "1" | "retry" | "r" => return Ok(RetryChoice::Retry),
                "2" | "exit" | "e" | "q" | "quit" => return Ok(RetryChoice::Exit),
                "" => self.warn("Please choose 1 or 2."),
                _ => self.warn("Unrecognized choice. Enter 1 to retry or 2 to exit."),
            }
        }
    }

    fn prompt_yes_no(&self, message: &str) -> AppResult<bool> {
        loop {
            let choice = self.prompt_line(message)?;
            match choice.trim().to_ascii_lowercase().as_str() {
                "y" | "yes" => return Ok(true),
                "n" | "no" => return Ok(false),
                "" => self.warn("Please answer y or n."),
                _ => self.warn("Please answer y or n."),
            }
        }
    }

    fn prompt_continue_or_exit(&self, message: &str) -> AppResult<bool> {
        loop {
            let choice = self.prompt_line(message)?;
            match choice.trim().to_ascii_lowercase().as_str() {
                "" | "continue" | "c" | "done" | "ready" => return Ok(true),
                "exit" | "quit" | "q" | "skip" | "cancel" => return Ok(false),
                _ => self.warn("Press Enter when ready, or type exit."),
            }
        }
    }

    fn paint(&self, text: &str, tone: Tone) -> String {
        if !self.color {
            return text.to_string();
        }

        match tone {
            Tone::Banner => format!("{}", text.bright_blue().bold()),
            Tone::BannerTitle => format!("{}", text.bright_white().bold()),
            Tone::BannerText => format!("{}", text.cyan()),
            Tone::Section => format!("{}", text.bright_blue().bold()),
            Tone::StepPrefix => format!("{}", text.bright_blue().bold()),
            Tone::StepText => format!("{}", text.bold()),
            Tone::InfoPrefix => format!("{}", text.bright_cyan().bold()),
            Tone::SuccessPrefix => format!("{}", text.bright_green().bold()),
            Tone::WarnPrefix => format!("{}", text.bright_yellow().bold()),
            Tone::HintPrefix => format!("{}", text.bright_magenta().bold()),
            Tone::ErrorPrefix => format!("{}", text.bright_red().bold()),
            Tone::ErrorTitle => format!("{}", text.bright_red().bold()),
            Tone::Detail => format!("{}", text.dimmed()),
            Tone::Prompt => format!("{}", text.bright_white().bold()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Tone {
    Banner,
    BannerTitle,
    BannerText,
    Section,
    StepPrefix,
    StepText,
    InfoPrefix,
    SuccessPrefix,
    WarnPrefix,
    HintPrefix,
    ErrorPrefix,
    ErrorTitle,
    Detail,
    Prompt,
}

#[derive(Debug, Clone, Copy)]
enum RetryChoice {
    Retry,
    Exit,
}

#[derive(Debug)]
struct JavaRuntime {
    command: PathBuf,
    major: u32,
    version_line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JavaInstallChoice {
    Install,
    Decline,
}

fn main() {
    let ui = Ui::new();
    let exit_code = match run(&ui) {
        Ok(()) => 0,
        Err(error) => {
            ui.error_report(&error);
            1
        }
    };

    pause_before_exit(&ui);
    std::process::exit(exit_code);
}

fn run(ui: &Ui) -> AppResult<()> {
    let args = parse_args()?;
    if args.show_help {
        print_help();
        return Ok(());
    }

    ui.banner();

    let client = build_http_client()?;
    let preflight = preflight_checks(ui, &client, &args)?;

    ui.section("Install");
    if !args.skip_fabric {
        if let Some(asset) = &preflight.manifest.assets.fabric_installer {
            install_fabric(
                ui,
                &client,
                &preflight.temp_dir,
                asset,
                preflight
                    .java_command
                    .as_deref()
                    .unwrap_or_else(|| Path::new("java")),
                &preflight.minecraft_dir,
                &preflight.manifest.minecraft_version,
            )?;
        } else {
            ui.info("No Fabric installer asset was included in this release. Skipping Fabric.");
        }
    } else {
        ui.info("Skipping Fabric installation because --skip-fabric was supplied.");
    }

    let mut changed_folders = Vec::new();

    sync_pack(
        ui,
        &client,
        &preflight.temp_dir,
        "mods",
        &preflight.manifest.assets.mods,
        &preflight.minecraft_dir,
        args.skip_mods,
        &mut changed_folders,
    )?;
    sync_pack(
        ui,
        &client,
        &preflight.temp_dir,
        "resourcepacks",
        &preflight.manifest.assets.resourcepacks,
        &preflight.minecraft_dir,
        args.skip_resourcepacks,
        &mut changed_folders,
    )?;
    sync_pack(
        ui,
        &client,
        &preflight.temp_dir,
        "shaderpacks",
        &preflight.manifest.assets.shaderpacks,
        &preflight.minecraft_dir,
        args.skip_shaderpacks,
        &mut changed_folders,
    )?;

    println!();
    ui.success(format!(
        "Sync completed for manifest version {}.",
        preflight.manifest.version
    ));
    ui.hint("You can now launch your Fabric profile in Minecraft.");
    Ok(())
}

fn preflight_checks(ui: &Ui, client: &Client, args: &Args) -> AppResult<PreflightResult> {
    ui.section("Preflight Checks");

    ui.step("Finding your Minecraft folder");
    let minecraft_dir = resolve_minecraft_dir(ui, args.minecraft_dir.clone())?;
    ui.success(format!(
        "Using Minecraft directory: {}",
        minecraft_dir.display()
    ));

    let java_command = if !args.skip_fabric {
        Some(check_java_21_plus(ui, client)?)
    } else {
        ui.info("Skipping Java preflight because --skip-fabric was supplied.");
        None
    };

    let manifest = run_with_retry(ui, "Retry manifest check", || {
        check_manifest_reachable(ui, client, &args.manifest_url)
    })?;
    ui.success(format!(
        "Found release {} for Minecraft {}.",
        manifest.release_tag, manifest.minecraft_version
    ));

    check_target_writable(ui, &minecraft_dir)?;
    let temp_dir = create_temp_dir(ui)?;

    Ok(PreflightResult {
        manifest,
        minecraft_dir,
        temp_dir,
        java_command,
    })
}

fn parse_args() -> AppResult<Args> {
    let mut args = Args {
        manifest_url: DEFAULT_MANIFEST_URL.to_string(),
        minecraft_dir: None,
        skip_fabric: false,
        skip_mods: false,
        skip_resourcepacks: false,
        skip_shaderpacks: false,
        show_help: false,
    };

    let mut iter = env::args_os().skip(1);

    while let Some(arg) = iter.next() {
        match arg.to_string_lossy().as_ref() {
            "--manifest-url" => {
                let value = next_value(&mut iter, "--manifest-url")?;
                args.manifest_url = value.into_string().map_err(|_| {
                    AppError::new(
                        ErrorKind::Other,
                        "The manifest URL must be valid UTF-8 text.",
                    )
                })?;
            }
            "--minecraft-dir" => {
                let value = next_value(&mut iter, "--minecraft-dir")?;
                args.minecraft_dir = Some(PathBuf::from(value));
            }
            "--skip-fabric" => args.skip_fabric = true,
            "--skip-mods" => args.skip_mods = true,
            "--skip-resourcepacks" => args.skip_resourcepacks = true,
            "--skip-shaderpacks" => args.skip_shaderpacks = true,
            "--help" | "-h" => args.show_help = true,
            other => {
                return Err(AppError::new(
                    ErrorKind::Other,
                    format!("I did not understand the option `{other}`."),
                )
                .with_suggestion("Run `minecraft-sync --help` to see the available options."));
            }
        }
    }

    Ok(args)
}

fn next_value(iter: &mut impl Iterator<Item = OsString>, flag: &str) -> AppResult<OsString> {
    iter.next().ok_or_else(|| {
        AppError::new(
            ErrorKind::Other,
            format!("The option `{flag}` needs a value after it."),
        )
    })
}

fn print_help() {
    println!("{}", help_text());
}

fn help_text() -> String {
    format!(
        "Usage: minecraft-sync [options]

Guided installer for the Minecraft Sync modpack.
It checks Java {MIN_JAVA_VERSION}+, verifies the release manifest, and safely syncs mods, resourcepacks, and shaderpacks.

Options:
  --manifest-url <url>   Override the release manifest URL
  --minecraft-dir <dir>  Override the detected Minecraft directory
  --skip-fabric          Skip Fabric installation and Java preflight
  --skip-mods            Skip mods sync
  --skip-resourcepacks   Skip resourcepacks sync
  --skip-shaderpacks     Skip shaderpacks sync
  --help, -h             Show this help text

If Minecraft is not found in the default location, interactive sessions will offer a guided path recovery menu."
    )
}

fn build_http_client() -> AppResult<Client> {
    Client::builder()
        .user_agent("minecraft-sync-installer")
        .build()
        .map_err(|error| {
            AppError::new(
                ErrorKind::Other,
                "The installer could not start its network client.",
            )
            .with_suggestion("Try running the installer again.")
            .with_detail(error.to_string())
        })
}

fn resolve_minecraft_dir(ui: &Ui, cli_path: Option<PathBuf>) -> AppResult<PathBuf> {
    if let Some(path) = cli_path {
        return resolve_explicit_minecraft_dir(&path);
    }

    let default_dir = detect_minecraft_dir()?;
    if default_dir.is_dir() {
        return Ok(default_dir);
    }

    if default_dir.exists() && !default_dir.is_dir() {
        return Err(AppError::new(
            ErrorKind::InvalidMinecraftDir,
            format!(
                "The default Minecraft path exists, but it is not a folder: {}",
                default_dir.display()
            ),
        )
        .with_suggestion("Open Minecraft once to recreate the folder, or choose a custom path."));
    }

    recover_missing_minecraft_dir(ui, &default_dir)
}

fn detect_minecraft_dir() -> AppResult<PathBuf> {
    let os = env::consts::OS;
    match os {
        "windows" => {
            let appdata = env::var_os("APPDATA").ok_or_else(|| {
                AppError::new(
                    ErrorKind::InvalidMinecraftDir,
                    "Windows did not provide an APPDATA folder for Minecraft detection.",
                )
                .with_suggestion(
                    "Open a terminal from your normal Windows user account and try again.",
                )
                .with_suggestion("Or rerun the installer with --minecraft-dir <path>.")
            })?;
            Ok(PathBuf::from(appdata).join(".minecraft"))
        }
        "macos" => Ok(home_dir()?.join("Library/Application Support/minecraft")),
        "linux" => Ok(home_dir()?.join(".minecraft")),
        other => Err(AppError::new(
            ErrorKind::UnsupportedOs,
            format!("This installer does not support `{other}` yet."),
        )),
    }
}

fn home_dir() -> AppResult<PathBuf> {
    env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        AppError::new(
            ErrorKind::InvalidMinecraftDir,
            "The installer could not find your home directory.",
        )
        .with_suggestion("Open the installer from your normal user account and try again.")
    })
}

fn recover_missing_minecraft_dir(ui: &Ui, default_dir: &Path) -> AppResult<PathBuf> {
    if !ui.can_prompt() {
        return Err(AppError::new(
            ErrorKind::InvalidMinecraftDir,
            format!(
                "Minecraft was not found at the default location: {}",
                default_dir.display()
            ),
        )
        .with_suggestion("Open Minecraft once so it creates the folder, then rerun the installer.")
        .with_suggestion("Or rerun with --minecraft-dir <path> to point at your custom folder."));
    }

    ui.warn(format!(
        "Minecraft was not found at the default location: {}",
        default_dir.display()
    ));
    ui.hint(
        "You can paste the .minecraft folder, its parent folder, or a common subfolder like mods.",
    );

    loop {
        println!();
        println!("Choose an option:");
        println!("  1) Enter Minecraft path");
        println!("  2) Exit");

        let choice = ui.prompt_line("Selection: ")?;
        match choice.trim().to_ascii_lowercase().as_str() {
            "1" | "path" | "p" => {
                let raw_path = ui.prompt_line("Enter the Minecraft folder path: ")?;
                if raw_path.trim().is_empty() {
                    ui.warn("Please enter a path or choose Exit.");
                    continue;
                }

                match resolve_prompted_minecraft_dir(Path::new(raw_path.trim())) {
                    Ok(path) => return Ok(path),
                    Err(error) => {
                        ui.recoverable_error(&error);
                    }
                }
            }
            "2" | "exit" | "e" | "q" | "quit" => {
                return Err(AppError::new(
                    ErrorKind::UserCancelled,
                    "Installation cancelled because no Minecraft directory was selected.",
                ));
            }
            "" => ui.warn("Please choose 1 or 2."),
            _ => ui.warn("Unrecognized choice. Enter 1 to supply a path or 2 to exit."),
        }
    }
}

fn resolve_explicit_minecraft_dir(raw_path: &Path) -> AppResult<PathBuf> {
    let normalized = normalize_user_path(raw_path)?;
    ensure_existing_directory(&normalized)?;

    if is_probable_minecraft_dir(&normalized) || looks_named_like_minecraft(&normalized) {
        return Ok(normalized);
    }

    Err(AppError::new(
        ErrorKind::InvalidMinecraftDir,
        format!(
            "The supplied folder does not look like a Minecraft directory: {}",
            normalized.display()
        ),
    )
    .with_suggestion("Point --minecraft-dir at the .minecraft folder itself, or use one that already contains Minecraft files."))
}

fn resolve_prompted_minecraft_dir(raw_path: &Path) -> AppResult<PathBuf> {
    let normalized = normalize_user_path(raw_path)?;
    let candidates = candidate_minecraft_dirs(&normalized);

    for candidate in &candidates {
        if is_probable_minecraft_dir(candidate) {
            return Ok(candidate.clone());
        }
    }

    if normalized.exists() && !normalized.is_dir() {
        return Err(AppError::new(
            ErrorKind::InvalidMinecraftDir,
            format!(
                "That path exists, but it is not a folder: {}",
                normalized.display()
            ),
        )
        .with_suggestion("Paste the folder path, not a file inside it."));
    }

    if let Some(candidate) = candidates
        .iter()
        .find(|candidate| candidate.is_dir() && looks_named_like_minecraft(candidate))
    {
        return Err(AppError::new(
            ErrorKind::InvalidMinecraftDir,
            format!(
                "I found a folder named like Minecraft at {}, but it does not contain enough Minecraft files yet.",
                candidate.display()
            ),
        )
        .with_suggestion("Launch Minecraft once with that launcher, then try the installer again.")
        .with_suggestion("Or paste a folder that already contains files like versions, assets, or options.txt."));
    }

    if normalized.exists() {
        return Err(AppError::new(
            ErrorKind::InvalidMinecraftDir,
            format!(
                "That folder exists, but it does not look like a Minecraft directory: {}",
                normalized.display()
            ),
        )
        .with_suggestion(
            "Paste the .minecraft folder itself, or a parent folder that contains it.",
        ));
    }

    Err(AppError::new(
        ErrorKind::InvalidMinecraftDir,
        format!("I could not find a Minecraft folder from: {}", normalized.display()),
    )
    .with_suggestion("Paste the .minecraft folder, its parent folder, or a folder like mods or versions inside it."))
}

fn ensure_existing_directory(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::new(
            ErrorKind::InvalidMinecraftDir,
            format!("That folder does not exist: {}", path.display()),
        )
        .with_suggestion("Double-check the path and try again."));
    }

    if !path.is_dir() {
        return Err(AppError::new(
            ErrorKind::InvalidMinecraftDir,
            format!("That path is not a folder: {}", path.display()),
        )
        .with_suggestion("Paste the Minecraft folder path, not a file."));
    }

    Ok(())
}

fn normalize_user_path(raw_path: &Path) -> AppResult<PathBuf> {
    let env_vars: HashMap<String, String> = env::vars().collect();
    let home = home_dir()?;
    let cwd =
        env::current_dir().map_err(|error| io_error(error, "read the current folder", None))?;

    normalize_user_path_for(&raw_path.to_string_lossy(), &home, &cwd, &env_vars)
}

fn normalize_user_path_for(
    raw_input: &str,
    home: &Path,
    cwd: &Path,
    env_vars: &HashMap<String, String>,
) -> AppResult<PathBuf> {
    let mut value = strip_wrapping_quotes(raw_input.trim()).trim().to_string();
    value = expand_env_tokens_with(&value, env_vars);

    if let Some(stripped) = value.strip_prefix("~/") {
        value = home.join(stripped).to_string_lossy().into_owned();
    } else if value == "~" {
        value = home.to_string_lossy().into_owned();
    }

    let looks_windows_absolute = looks_like_windows_absolute(&value);
    let path = PathBuf::from(value);
    if path.is_absolute() || looks_windows_absolute {
        Ok(path)
    } else {
        Ok(cwd.join(path))
    }
}

fn strip_wrapping_quotes(value: &str) -> &str {
    if value.len() >= 2 {
        let first = value.as_bytes()[0];
        let last = value.as_bytes()[value.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &value[1..value.len() - 1];
        }
    }

    value
}

fn expand_env_tokens_with(value: &str, env_vars: &HashMap<String, String>) -> String {
    let mut expanded = value.to_string();
    let mut entries: Vec<_> = env_vars.iter().collect();
    entries.sort_by(|(left, _), (right, _)| right.len().cmp(&left.len()));

    for (key, var_value) in entries {
        let percent_token = format!("%{key}%");
        if expanded.contains(&percent_token) {
            expanded = expanded.replace(&percent_token, var_value);
        }

        let braced_token = format!("${{{key}}}");
        if expanded.contains(&braced_token) {
            expanded = expanded.replace(&braced_token, var_value);
        }

        let plain_token = format!("${key}");
        if expanded.contains(&plain_token) {
            expanded = expanded.replace(&plain_token, var_value);
        }
    }

    expanded
}

fn candidate_minecraft_dirs(path: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    push_candidate(&mut candidates, path.to_path_buf());

    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
        match name.to_ascii_lowercase().as_str() {
            "mods" | "resourcepacks" | "shaderpacks" | "saves" | "versions" => {
                if let Some(parent) = path.parent() {
                    push_candidate(&mut candidates, parent.to_path_buf());
                }
            }
            _ => {}
        }
    }

    push_candidate(&mut candidates, path.join(".minecraft"));
    push_candidate(&mut candidates, path.join("minecraft"));
    push_candidate(
        &mut candidates,
        path.join("Library")
            .join("Application Support")
            .join("minecraft"),
    );
    push_candidate(
        &mut candidates,
        path.join("AppData").join("Roaming").join(".minecraft"),
    );

    candidates
}

fn push_candidate(candidates: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

fn looks_like_windows_absolute(value: &str) -> bool {
    let trimmed = value.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        return true;
    }

    trimmed.starts_with("\\\\")
}

fn looks_named_like_minecraft(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            let lower = name.to_ascii_lowercase();
            lower == ".minecraft" || lower == "minecraft"
        })
        .unwrap_or(false)
}

fn is_probable_minecraft_dir(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }

    has_minecraft_markers(path)
}

fn has_minecraft_markers(path: &Path) -> bool {
    let markers = [
        "mods",
        "resourcepacks",
        "shaderpacks",
        "versions",
        "assets",
        "launcher_profiles.json",
        "options.txt",
    ];

    markers.iter().any(|marker| path.join(marker).exists())
}

fn check_java_21_plus(ui: &Ui, client: &Client) -> AppResult<PathBuf> {
    loop {
        ui.step(format!("Checking Java {MIN_JAVA_VERSION}+"));

        match probe_java_runtime() {
            Ok(runtime) => {
                if runtime.major < MIN_JAVA_VERSION {
                    return Err(AppError::new(
                        ErrorKind::JavaVersionTooOld,
                        format!(
                            "Java {} is installed, but this modpack requires Java {MIN_JAVA_VERSION} or newer.",
                            runtime.major
                        ),
                    )
                    .with_suggestion("Install Java 21 or newer, then rerun the installer.")
                    .with_detail(runtime.version_line));
                }

                ui.success(format!("Java {} is installed.", runtime.major));
                return Ok(runtime.command);
            }
            Err(error) if error.kind == ErrorKind::JavaMissing && ui.can_prompt() => {
                ui.recoverable_error(&error);
                match prompt_for_java_install(ui)? {
                    JavaInstallChoice::Install => {
                        install_or_guide_java(ui, client)?;
                        continue;
                    }
                    JavaInstallChoice::Decline => return Err(error),
                }
            }
            Err(error) => return Err(error),
        }
    }
}

fn parse_java_major(output: &str) -> Option<u32> {
    let start = output.find('"')?;
    let rest = &output[start + 1..];
    let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn probe_java_runtime() -> AppResult<JavaRuntime> {
    let mut saw_missing = false;

    for candidate in java_command_candidates() {
        match run_java_version(&candidate) {
            Ok(runtime) => return Ok(runtime),
            Err(error) if error.kind == ErrorKind::JavaMissing => {
                saw_missing = true;
            }
            Err(error) => return Err(error),
        }
    }

    if saw_missing {
        Err(AppError::new(
            ErrorKind::JavaMissing,
            "Java was not found on this computer.",
        )
        .with_suggestion("Install Java 21 or newer, then run the installer again.")
        .with_suggestion(
            "If Java is already installed, make sure the `java` command is available in PATH.",
        ))
    } else {
        Err(AppError::new(
            ErrorKind::JavaMissing,
            "Java was not found on this computer.",
        ))
    }
}

fn run_java_version(command: &Path) -> AppResult<JavaRuntime> {
    let output = Command::new(command)
        .arg("-version")
        .output()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                AppError::new(
                    ErrorKind::JavaMissing,
                    format!("Java was not found at {}.", command.display()),
                )
            } else {
                io_error(error, "run `java -version`", Some(command))
                    .with_suggestion("Install Java 21 or newer, then run the installer again.")
            }
        })?;

    let combined_output = combined_command_output(&output.stdout, &output.stderr);
    let major = parse_java_major(&combined_output).ok_or_else(|| {
        AppError::new(
            ErrorKind::JavaVersionTooOld,
            "Java was found, but the installer could not understand its version.",
        )
        .with_suggestion("Install Java 21 or newer from Oracle or OpenJDK, then try again.")
        .with_detail(first_line(&combined_output))
    })?;

    Ok(JavaRuntime {
        command: command.to_path_buf(),
        major,
        version_line: first_line(&combined_output),
    })
}

fn java_command_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("java")];

    match env::consts::OS {
        "windows" => {
            if let Some(program_files) = env::var_os("ProgramFiles") {
                add_java_candidates_from_dir(
                    &mut candidates,
                    &PathBuf::from(program_files).join("Java"),
                    "bin/java.exe",
                );
            }
        }
        "macos" => {
            add_java_candidates_from_dir(
                &mut candidates,
                Path::new("/Library/Java/JavaVirtualMachines"),
                "Contents/Home/bin/java",
            );
        }
        _ => {}
    }

    candidates
}

fn add_java_candidates_from_dir(candidates: &mut Vec<PathBuf>, base_dir: &Path, suffix: &str) {
    let Ok(entries) = fs::read_dir(base_dir) else {
        return;
    };

    let mut discovered = Vec::new();
    for entry in entries.flatten() {
        discovered.push(entry.path().join(suffix));
    }
    discovered.sort();
    discovered.reverse();

    for candidate in discovered {
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
}

fn prompt_for_java_install(ui: &Ui) -> AppResult<JavaInstallChoice> {
    if ui.prompt_yes_no("Would you like help installing Java now? [y/n]: ")? {
        Ok(JavaInstallChoice::Install)
    } else {
        Ok(JavaInstallChoice::Decline)
    }
}

fn install_or_guide_java(ui: &Ui, client: &Client) -> AppResult<()> {
    match env::consts::OS {
        "windows" => download_and_run_windows_java_installer(ui, client),
        "macos" => download_and_open_macos_java_installer(ui, client),
        "linux" => guide_linux_java_install(ui),
        other => Err(AppError::new(
            ErrorKind::UnsupportedOs,
            format!("Automatic Java help is not supported on {other}."),
        )
        .with_suggestion("Install Java 21 or newer manually, then rerun the installer.")),
    }
}

fn download_and_run_windows_java_installer(ui: &Ui, client: &Client) -> AppResult<()> {
    ui.step("Downloading the Windows Java installer");
    let download_path = preferred_download_path("jdk-26_windows-x64_bin.msi")?;
    download_support_file(
        client,
        WINDOWS_JAVA_INSTALLER_URL,
        &download_path,
        "the Windows Java installer",
    )?;
    ui.success(format!(
        "Downloaded the Windows Java installer to {}.",
        download_path.display()
    ));

    ui.step("Launching the Windows Java installer");
    let status = Command::new("msiexec")
        .arg("/i")
        .arg(&download_path)
        .status()
        .map_err(|error| {
            io_error(
                error,
                "start the Windows Java installer",
                Some(&download_path),
            )
        })?;

    if !status.success() {
        return Err(AppError::new(
            ErrorKind::JavaMissing,
            "The Windows Java installer did not finish successfully.",
        )
        .with_suggestion("Complete the MSI installer, then run Minecraft Sync again.")
        .with_detail(format!("Installer exit status: {status}")));
    }

    ui.success("The Java MSI installer finished.");
    Ok(())
}

fn download_and_open_macos_java_installer(ui: &Ui, client: &Client) -> AppResult<()> {
    ui.step("Downloading the macOS Java installer");
    let download_path = preferred_download_path("jdk-26_macos-x64_bin.dmg")?;
    download_support_file(
        client,
        MACOS_JAVA_INSTALLER_URL,
        &download_path,
        "the macOS Java installer",
    )?;
    ui.success(format!(
        "Downloaded the macOS Java installer to {}.",
        download_path.display()
    ));

    ui.step("Opening the macOS Java installer");
    let status = Command::new("open")
        .arg(&download_path)
        .status()
        .map_err(|error| io_error(error, "open the macOS Java installer", Some(&download_path)))?;

    if !status.success() {
        return Err(AppError::new(
            ErrorKind::JavaMissing,
            "The macOS Java installer could not be opened automatically.",
        )
        .with_suggestion(format!(
            "Open {} manually and complete the installer.",
            download_path.display()
        ))
        .with_detail(format!("Installer exit status: {status}")));
    }

    ui.hint("Finish the Java installer in macOS, then come back here.");
    if !ui.prompt_continue_or_exit("Press Enter after Java is installed, or type exit: ")? {
        return Err(AppError::new(
            ErrorKind::UserCancelled,
            "Installation cancelled while waiting for Java setup.",
        ));
    }

    Ok(())
}

fn guide_linux_java_install(ui: &Ui) -> AppResult<()> {
    ui.step("Preparing Linux Java installation guidance");
    let command = detect_linux_java_install_command().ok_or_else(|| {
        AppError::new(
            ErrorKind::JavaMissing,
            "Java was not found, and the installer could not recognize your Linux package manager.",
        )
        .with_suggestion("Install OpenJDK 21 or newer manually, then rerun the installer.")
    })?;

    ui.hint("Run this command in another terminal to install Java:");
    println!("{command}");
    if !ui.prompt_continue_or_exit("Press Enter after Java is installed, or type exit: ")? {
        return Err(AppError::new(
            ErrorKind::UserCancelled,
            "Installation cancelled while waiting for Java setup.",
        ));
    }

    Ok(())
}

fn detect_linux_java_install_command() -> Option<&'static str> {
    if command_exists("apt-get") {
        Some("sudo apt-get update && sudo apt-get install -y openjdk-21-jdk")
    } else if command_exists("dnf") {
        Some("sudo dnf install -y java-21-openjdk")
    } else if command_exists("pacman") {
        Some("sudo pacman -S --needed jdk-openjdk")
    } else {
        None
    }
}

fn command_exists(command: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| {
            env::split_paths(&paths).any(|path| {
                let full_path = path.join(command);
                full_path.is_file()
            })
        })
        .unwrap_or(false)
}

fn preferred_download_path(file_name: &str) -> AppResult<PathBuf> {
    let downloads_dir = downloads_dir()?;
    fs::create_dir_all(&downloads_dir)
        .map_err(|error| io_error(error, "create the Downloads folder", Some(&downloads_dir)))?;
    Ok(downloads_dir.join(file_name))
}

fn downloads_dir() -> AppResult<PathBuf> {
    match env::consts::OS {
        "windows" => {
            let profile = env::var_os("USERPROFILE").ok_or_else(|| {
                AppError::new(
                    ErrorKind::Other,
                    "The installer could not locate your Windows profile folder.",
                )
            })?;
            Ok(PathBuf::from(profile).join("Downloads"))
        }
        "macos" | "linux" => Ok(home_dir()?.join("Downloads")),
        other => Err(AppError::new(
            ErrorKind::UnsupportedOs,
            format!("Download folder detection is not supported on {other}."),
        )),
    }
}

fn download_support_file(
    client: &Client,
    url: &str,
    destination: &Path,
    label: &str,
) -> AppResult<()> {
    let response = client.get(url).send().map_err(|error| {
        network_error(
            &format!("The installer could not download {label}."),
            url,
            error,
        )
    })?;

    if !response.status().is_success() {
        return Err(http_status_error(
            &format!("The installer could not download {label}."),
            url,
            response.status(),
        ));
    }

    let bytes = response.bytes().map_err(|error| {
        AppError::new(
            ErrorKind::Network,
            format!("The download for {label} started, but it did not finish cleanly."),
        )
        .with_suggestion("Check your internet connection and try again.")
        .with_detail(error.to_string())
    })?;

    fs::write(destination, &bytes)
        .map_err(|error| io_error(error, "write a downloaded installer", Some(destination)))?;
    Ok(())
}

fn check_manifest_reachable(ui: &Ui, client: &Client, manifest_url: &str) -> AppResult<Manifest> {
    ui.step("Checking internet connection and release manifest");

    let response = client.get(manifest_url).send().map_err(|error| {
        network_error(
            "The installer could not download the release manifest.",
            manifest_url,
            error,
        )
    })?;

    if !response.status().is_success() {
        return Err(http_status_error(
            "The release manifest could not be downloaded.",
            manifest_url,
            response.status(),
        ));
    }

    let body = response.text().map_err(|error| {
        AppError::new(
            ErrorKind::Network,
            "The release manifest started downloading, but the download could not finish.",
        )
        .with_suggestion("Check your internet connection and try again.")
        .with_detail(error.to_string())
    })?;

    serde_json::from_str::<Manifest>(&body).map_err(|error| {
        AppError::new(
            ErrorKind::ManifestParse,
            "The release manifest was downloaded, but it could not be read.",
        )
        .with_suggestion("Try again in a minute in case the release is still publishing.")
        .with_suggestion(
            "If the problem keeps happening, inspect the manifest asset on GitHub Releases.",
        )
        .with_detail(error.to_string())
    })
}

fn check_target_writable(ui: &Ui, path: &Path) -> AppResult<()> {
    ui.step("Checking write access to your Minecraft folder");
    ensure_target_writable(path)?;
    ui.success("Minecraft folder is writable.");
    Ok(())
}

fn ensure_target_writable(path: &Path) -> AppResult<()> {
    ensure_existing_directory(path)?;

    TempFileBuilder::new()
        .prefix(".minecraft-sync-write-test")
        .tempfile_in(path)
        .map(|_| ())
        .map_err(|error| {
            if error.kind() == io::ErrorKind::PermissionDenied {
                AppError::new(
                    ErrorKind::PermissionDenied,
                    format!(
                        "The installer does not have permission to write inside {}.",
                        path.display()
                    ),
                )
                .with_suggestion("Run the installer from a user account that owns this folder.")
                .with_suggestion("Close Minecraft or your launcher if it is locking the folder.")
                .with_detail(error.to_string())
            } else {
                io_error(
                    error,
                    &format!("create a temporary file inside {}", path.display()),
                    Some(path),
                )
            }
        })
}

fn create_temp_dir(ui: &Ui) -> AppResult<TempDir> {
    ui.step("Preparing a temporary workspace");
    let temp_dir = TempDir::new().map_err(|error| {
        AppError::new(
            ErrorKind::TempDir,
            "The installer could not create a temporary working folder.",
        )
        .with_suggestion("Make sure your system temp folder is writable and has free space.")
        .with_detail(error.to_string())
    })?;
    ui.success(format!(
        "Temporary workspace ready at {}.",
        temp_dir.path().display()
    ));
    Ok(temp_dir)
}

fn run_with_retry<T, F>(ui: &Ui, retry_label: &str, mut action: F) -> AppResult<T>
where
    F: FnMut() -> AppResult<T>,
{
    loop {
        match action() {
            Ok(value) => return Ok(value),
            Err(error) => {
                if !error.kind.is_retryable() || !ui.can_prompt() {
                    return Err(error);
                }

                ui.recoverable_error(&error);
                match ui.prompt_retry_or_exit(retry_label)? {
                    RetryChoice::Retry => continue,
                    RetryChoice::Exit => {
                        return Err(AppError::new(
                            ErrorKind::UserCancelled,
                            "Installation cancelled by user.",
                        ));
                    }
                }
            }
        }
    }
}

fn install_fabric(
    ui: &Ui,
    client: &Client,
    temp_dir: &TempDir,
    asset: &ReleaseAsset,
    java_command: &Path,
    minecraft_dir: &Path,
    minecraft_version: &str,
) -> AppResult<()> {
    ui.step("Installing Fabric");
    let jar_path = download_asset(ui, client, temp_dir, "fabric-installer.jar", asset)?;

    let output = Command::new(java_command)
        .arg("-jar")
        .arg(&jar_path)
        .arg("client")
        .arg("-dir")
        .arg(minecraft_dir)
        .arg("-mcversion")
        .arg(minecraft_version)
        .output()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                AppError::new(
                    ErrorKind::JavaMissing,
                    "Java disappeared before Fabric could be installed.",
                )
                .with_suggestion("Install Java 21 or newer, then run the installer again.")
            } else {
                io_error(error, "start the Fabric installer", Some(java_command))
            }
        })?;

    if !output.status.success() {
        let detail = first_non_empty_line(&combined_command_output(&output.stdout, &output.stderr))
            .unwrap_or_else(|| "The Fabric installer exited with an error.".to_string());
        return Err(AppError::new(
            ErrorKind::FabricInstall,
            "Fabric could not be installed automatically.",
        )
        .with_suggestion("Make sure Java 21 or newer is installed.")
        .with_suggestion("Open Minecraft once with the target launcher, then rerun the installer.")
        .with_detail(detail));
    }

    ui.success("Fabric installed.");
    Ok(())
}

fn sync_pack(
    ui: &Ui,
    client: &Client,
    temp_dir: &TempDir,
    folder_name: &str,
    asset: &ReleaseAsset,
    minecraft_dir: &Path,
    skip: bool,
    changed_folders: &mut Vec<FolderState>,
) -> AppResult<()> {
    if skip {
        ui.info(format!(
            "Skipping {folder_name} because its flag was supplied."
        ));
        return Ok(());
    }

    ui.step(format!("Syncing {folder_name}"));
    let archive_path = download_asset(ui, client, temp_dir, &format!("{folder_name}.zip"), asset)?;
    let target_dir = minecraft_dir.join(folder_name);

    if target_dir.exists() {
        ui.info(format!("Backing up existing {folder_name} folder."));
    } else {
        ui.info(format!(
            "No existing {folder_name} folder was found. A new one will be created."
        ));
    }

    let backup = backup_folder(ui, &target_dir)?;
    changed_folders.push(FolderState {
        target: target_dir.clone(),
        backup,
    });

    if let Err(error) = extract_zip(&archive_path, &target_dir) {
        ui.warn(format!(
            "Something went wrong while updating {folder_name}. Restoring your previous files."
        ));
        return match rollback(ui, changed_folders) {
            Ok(()) => Err(AppError::new(
                ErrorKind::Other,
                format!(
                    "The installer could not finish syncing {folder_name}, but your previous files were restored."
                ),
            )
            .with_suggestion("You can try the installer again.")
            .with_detail(error.debug_summary())),
            Err(rollback_error) => Err(AppError::new(
                ErrorKind::Rollback,
                "The installer hit a problem and could not fully restore your previous files.",
            )
            .with_suggestion("Check the Minecraft folder before launching the game again.")
            .with_suggestion("Restore from your latest backup folder if needed.")
            .with_detail(format!(
                "Sync error: {} | Rollback error: {}",
                error.debug_summary(),
                rollback_error.debug_summary()
            ))),
        };
    }

    ui.success(format!("Synced {folder_name}."));
    Ok(())
}

fn download_asset(
    ui: &Ui,
    client: &Client,
    temp_dir: &TempDir,
    file_name: &str,
    asset: &ReleaseAsset,
) -> AppResult<PathBuf> {
    ui.info(format!("Downloading {file_name}."));
    let response = client.get(&asset.url).send().map_err(|error| {
        network_error(
            &format!("The installer could not download {file_name}."),
            &asset.url,
            error,
        )
    })?;

    if !response.status().is_success() {
        return Err(http_status_error(
            &format!("The installer could not download {file_name}."),
            &asset.url,
            response.status(),
        ));
    }

    let bytes = response.bytes().map_err(|error| {
        AppError::new(
            ErrorKind::Network,
            format!("The download for {file_name} started, but it did not finish cleanly."),
        )
        .with_suggestion("Check your internet connection and try again.")
        .with_detail(error.to_string())
    })?;

    if bytes.len() as u64 != asset.size {
        return Err(AppError::new(
            ErrorKind::AssetIntegrity,
            format!("The downloaded {file_name} did not match the expected size."),
        )
        .with_suggestion("Retry the installer so it can download a fresh copy.")
        .with_detail(format!(
            "Expected {} bytes, got {} bytes.",
            asset.size,
            bytes.len()
        )));
    }

    let digest = hex_sha256(bytes.as_ref());
    if digest != asset.sha256.to_ascii_lowercase() {
        return Err(AppError::new(
            ErrorKind::AssetIntegrity,
            format!("The downloaded {file_name} failed its SHA-256 integrity check."),
        )
        .with_suggestion("Retry the installer so it can download a fresh copy.")
        .with_detail(format!(
            "Expected SHA-256 {}, got {}.",
            asset.sha256, digest
        )));
    }

    let file_path = temp_dir.path().join(file_name);
    fs::write(&file_path, &bytes)
        .map_err(|error| io_error(error, "write a downloaded file", Some(&file_path)))?;
    Ok(file_path)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("{digest:x}")
}

fn backup_folder(ui: &Ui, target_dir: &Path) -> AppResult<Option<PathBuf>> {
    if !target_dir.exists() {
        return Ok(None);
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            AppError::new(
                ErrorKind::Other,
                "The system clock looks invalid for backup naming.",
            )
            .with_detail(error.to_string())
        })?
        .as_secs();
    let backup_path = target_dir.with_file_name(format!(
        "{}.bak.{timestamp}",
        target_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("minecraft-sync-backup")
    ));

    fs::rename(target_dir, &backup_path)
        .map_err(|error| io_error(error, "create a backup folder", Some(target_dir)))?;

    ui.success(format!("Backup created: {}", backup_path.display()));
    Ok(Some(backup_path))
}

fn extract_zip(archive_path: &Path, target_dir: &Path) -> AppResult<()> {
    fs::create_dir_all(target_dir)
        .map_err(|error| io_error(error, "create the destination folder", Some(target_dir)))?;

    let file = File::open(archive_path)
        .map_err(|error| io_error(error, "open the downloaded archive", Some(archive_path)))?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        AppError::new(
            ErrorKind::AssetIntegrity,
            format!("The archive {} could not be read.", archive_path.display()),
        )
        .with_detail(error.to_string())
    })?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            AppError::new(
                ErrorKind::AssetIntegrity,
                "A file inside the downloaded archive could not be read.",
            )
            .with_detail(error.to_string())
        })?;
        let enclosed_name = entry.enclosed_name().ok_or_else(|| {
            AppError::new(
                ErrorKind::AssetIntegrity,
                "A downloaded archive tried to extract files outside the Minecraft folder.",
            )
        })?;
        let output_path = target_dir.join(enclosed_name);

        if entry.name().ends_with('/') {
            fs::create_dir_all(&output_path).map_err(|error| {
                io_error(
                    error,
                    "create a folder from the archive",
                    Some(&output_path),
                )
            })?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                io_error(error, "create folders for extracted files", Some(parent))
            })?;
        }

        let mut output = File::create(&output_path)
            .map_err(|error| io_error(error, "create an extracted file", Some(&output_path)))?;
        io::copy(&mut entry, &mut output)
            .map_err(|error| io_error(error, "write extracted data", Some(&output_path)))?;
    }

    Ok(())
}

fn rollback(ui: &Ui, states: &[FolderState]) -> AppResult<()> {
    ui.step("Rolling back partial changes");
    for state in states.iter().rev() {
        if state.target.exists() {
            fs::remove_dir_all(&state.target).map_err(|error| {
                io_error(
                    error,
                    "remove a partially synced folder",
                    Some(&state.target),
                )
            })?;
        }

        if let Some(backup) = &state.backup {
            if backup.exists() {
                fs::rename(backup, &state.target)
                    .map_err(|error| io_error(error, "restore a backup folder", Some(backup)))?;
            }
        }
    }

    ui.success("Previous folder backups were restored.");
    Ok(())
}

fn pause_before_exit(ui: &Ui) {
    if env::consts::OS != "windows" || !ui.can_prompt() {
        return;
    }

    println!();
    println!("Press Enter to exit...");
    let _ = ui.prompt_line("");
}

fn combined_command_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);

    if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    }
}

fn first_line(text: &str) -> String {
    first_non_empty_line(text).unwrap_or_else(|| text.trim().to_string())
}

fn first_non_empty_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn network_error(message: &str, url: &str, error: reqwest::Error) -> AppError {
    let detail = error.to_string();
    let mut app_error = if error.is_timeout() {
        AppError::new(
            ErrorKind::Network,
            format!("{message} The request timed out."),
        )
        .with_suggestion("Check your internet connection and try again.")
    } else if error.is_connect() || looks_like_dns_issue(&detail) {
        AppError::new(
            ErrorKind::Network,
            format!("{message} The installer could not reach GitHub."),
        )
        .with_suggestion("Check your internet connection, VPN, proxy, or firewall settings.")
    } else {
        AppError::new(ErrorKind::Network, message)
            .with_suggestion("Check your internet connection and try again.")
    };

    app_error = app_error.with_suggestion(format!("Manifest or asset URL: {url}"));
    app_error.with_detail(detail)
}

fn http_status_error(message: &str, url: &str, status: StatusCode) -> AppError {
    let mut error = AppError::new(
        ErrorKind::Network,
        format!("{message} GitHub returned HTTP {}.", status.as_u16()),
    )
    .with_detail(format!("URL: {url}"));

    if status == StatusCode::NOT_FOUND {
        error = error
            .with_suggestion("The release may still be publishing. Wait a minute and retry.")
            .with_suggestion(
                "If this keeps happening, confirm the release assets exist on GitHub.",
            );
    } else {
        error = error
            .with_suggestion("Try again in a minute in case GitHub is having a temporary issue.");
    }

    error
}

fn looks_like_dns_issue(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("dns")
        || lower.contains("resolve")
        || lower.contains("name or service not known")
        || lower.contains("could not resolve host")
}

fn io_error(error: io::Error, action: &str, path: Option<&Path>) -> AppError {
    if error.kind() == io::ErrorKind::PermissionDenied {
        let message = match path {
            Some(path) => format!(
                "The installer does not have permission to {action} at {}.",
                path.display()
            ),
            None => format!("The installer does not have permission to {action}."),
        };

        AppError::new(ErrorKind::PermissionDenied, message)
            .with_suggestion("Close Minecraft or your launcher if it is using this folder.")
            .with_suggestion("Try again from a user account that owns the Minecraft folder.")
            .with_detail(error.to_string())
    } else {
        let message = match path {
            Some(path) => format!("The installer could not {action} at {}.", path.display()),
            None => format!("The installer could not {action}."),
        };

        AppError::new(ErrorKind::Other, message).with_detail(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_user_path_strips_quotes_and_expands_home() {
        let home = PathBuf::from("/home/tester");
        let cwd = PathBuf::from("/tmp/work");
        let env_vars = HashMap::new();

        let path =
            normalize_user_path_for("\"~/Games/.minecraft\"", &home, &cwd, &env_vars).unwrap();

        assert_eq!(path, home.join("Games/.minecraft"));
    }

    #[test]
    fn normalize_user_path_expands_env_tokens() {
        let home = PathBuf::from("/home/tester");
        let cwd = PathBuf::from("/tmp/work");
        let mut env_vars = HashMap::new();
        env_vars.insert(
            "APPDATA".to_string(),
            "C:/Users/Alex/AppData/Roaming".to_string(),
        );
        env_vars.insert("HOME".to_string(), "/home/tester".to_string());

        let windows_path =
            normalize_user_path_for("%APPDATA%/.minecraft", &home, &cwd, &env_vars).unwrap();
        let unix_path =
            normalize_user_path_for("${HOME}/.minecraft", &home, &cwd, &env_vars).unwrap();

        assert_eq!(
            windows_path,
            PathBuf::from("C:/Users/Alex/AppData/Roaming/.minecraft")
        );
        assert_eq!(unix_path, home.join(".minecraft"));
    }

    #[test]
    fn candidate_minecraft_dirs_recovers_parent_from_mods_folder() {
        let candidates = candidate_minecraft_dirs(Path::new("/games/custom/.minecraft/mods"));

        assert!(candidates.contains(&PathBuf::from("/games/custom/.minecraft/mods")));
        assert!(candidates.contains(&PathBuf::from("/games/custom/.minecraft")));
    }

    #[test]
    fn is_probable_minecraft_dir_checks_strong_markers() {
        let temp_dir = tempfile::tempdir().unwrap();
        let valid_dir = temp_dir.path().join(".minecraft");
        let empty_dir = temp_dir.path().join("empty");

        fs::create_dir_all(valid_dir.join("versions")).unwrap();
        fs::create_dir_all(&empty_dir).unwrap();

        assert!(is_probable_minecraft_dir(&valid_dir));
        assert!(!is_probable_minecraft_dir(&empty_dir));
    }

    #[test]
    fn parse_java_major_accepts_supported_versions() {
        let java_21 = r#"openjdk version "21.0.7" 2025-04-15"#;
        let java_22 = r#"java version "22" 2026-03-19"#;

        assert_eq!(parse_java_major(java_21), Some(21));
        assert_eq!(parse_java_major(java_22), Some(22));
    }

    #[test]
    fn parse_java_major_rejects_old_or_unreadable_versions() {
        let java_17 = r#"openjdk version "17.0.11" 2024-04-16"#;
        let invalid = "some custom java output";

        assert_eq!(parse_java_major(java_17), Some(17));
        assert_eq!(parse_java_major(invalid), None);
    }

    #[test]
    fn ensure_target_writable_accepts_directory_and_rejects_file_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("not-a-directory.txt");
        fs::write(&file_path, "hello").unwrap();

        assert!(ensure_target_writable(temp_dir.path()).is_ok());

        let error = ensure_target_writable(&file_path).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidMinecraftDir);
    }

    #[test]
    fn help_text_mentions_preflight_and_main_flags() {
        let help = help_text();

        assert!(help.contains("--minecraft-dir"));
        assert!(help.contains("--skip-fabric"));
        assert!(help.contains("Java 21+"));
        assert!(help.contains("guided path recovery"));
    }
}
