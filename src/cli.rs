use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "kshot",
    author = "Koresent <koresent@pm.me>",
    version,
    about = "Native Wayland screenshot and annotation tool matching Gruvbox Dark aesthetic",
    long_about = "A high-performance, Wayland-native replacement for Flameshot written in Rust.\nFeatures multi-output capture, fractional scaling precision, rich vector annotations,\nGruvbox Dark theme, and YAML configuration."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start interactive GUI screenshot and annotation session
    Gui(GuiArgs),
    /// Capture full screen or a specific monitor
    Screen(ScreenArgs),
    /// Capture a predefined rectangular area non-interactively
    Area(AreaArgs),
    /// Pin an image to the screen as a borderless stays-on-top window
    Pin(PinArgs),
    /// View or manage configuration
    Config(ConfigArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct GuiArgs {
    /// Save screenshot to specific path or directory
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Copy captured region directly to clipboard
    #[arg(short, long)]
    pub clipboard: bool,

    /// Delay in milliseconds before freezing the screen
    #[arg(short, long, default_value_t = 0)]
    pub delay: u64,

    /// Pre-select geometry region (e.g. 800x600+100+100)
    #[arg(short, long)]
    pub region: Option<String>,

    /// Write raw PNG bytes directly to stdout
    #[arg(long)]
    pub raw: bool,

    /// Immediately pin the cropped area to screen upon completion
    #[arg(long)]
    pub pin: bool,

    /// Automatically accept and export as soon as region is selected
    #[arg(long)]
    pub accept_on_select: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct ScreenArgs {
    /// Target monitor / output name (e.g. DP-1, HDMI-A-1)
    #[arg(short, long)]
    pub output: Option<String>,

    /// Copy capture to clipboard
    #[arg(short, long)]
    pub clipboard: bool,

    /// Save capture to file
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Delay in milliseconds before capture
    #[arg(short, long, default_value_t = 0)]
    pub delay: u64,

    /// Open capture in GUI editor for annotations
    #[arg(short, long)]
    pub edit: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct AreaArgs {
    /// Target geometry (e.g. 1920x1080+0+0)
    #[arg(short, long)]
    pub geometry: String,

    /// Copy capture to clipboard
    #[arg(short, long)]
    pub clipboard: bool,

    /// Save capture to file
    #[arg(short, long)]
    pub path: Option<PathBuf>,
}

#[derive(Args, Debug, Clone, Default)]
pub struct PinArgs {
    /// Path to image file to pin
    #[arg(short, long)]
    pub file: PathBuf,
}

#[derive(Args, Debug, Clone, Default)]
pub struct ConfigArgs {
    /// Print path to configuration file
    #[arg(long)]
    pub path: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_gui() {
        let args = Cli::parse_from(["kshot", "gui", "-c", "-d", "100"]);
        if let Some(Commands::Gui(gui)) = args.command {
            assert!(gui.clipboard);
            assert_eq!(gui.delay, 100);
        } else {
            panic!("Expected Gui command");
        }
    }

    #[test]
    fn test_cli_parsing_screen() {
        let args = Cli::parse_from(["kshot", "screen", "-o", "DP-1", "-e"]);
        if let Some(Commands::Screen(screen)) = args.command {
            assert_eq!(screen.output.as_deref(), Some("DP-1"));
            assert!(screen.edit);
        } else {
            panic!("Expected Screen command");
        }
    }
}
