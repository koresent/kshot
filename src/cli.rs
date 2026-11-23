use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "kshot",
    author = "Koresent <koresent@pm.me>",
    version,
    about = "Native Wayland screenshot and annotation tool matching Gruvbox Dark aesthetic",
    long_about = "A high-performance, Wayland-native screenshot suite written in Rust.\nFeatures multi-output capture, fractional scaling precision, rich vector annotations,\nGruvbox Dark theme, window selection, scrolling capture, and native HUD popover."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start interactive GUI screenshot and annotation session
    Gui(GuiArgs),
    /// Open native HUD mode selector overlay
    Hud(HudArgs),
    /// Interactive window screenshot with hover highlight
    Window(WindowArgs),
    /// Vertical scrolling screenshot with real-time stitching
    Scroll(ScrollArgs),
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

    /// Apply rounded corners and soft drop shadow
    #[arg(long)]
    pub pretty: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct HudArgs {
    /// Open HUD with pretty mode active by default
    #[arg(long)]
    pub pretty: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct WindowArgs {
    /// Copy capture to clipboard (default behavior)
    #[arg(short, long, default_value_t = true)]
    pub clipboard: bool,

    /// Save capture to file
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Open capture in GUI editor for annotations
    #[arg(short, long)]
    pub edit: bool,

    /// Apply rounded corners and soft drop shadow
    #[arg(long)]
    pub pretty: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct ScrollArgs {
    /// Copy capture to clipboard
    #[arg(short, long, default_value_t = true)]
    pub clipboard: bool,

    /// Save capture to file
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Open capture in GUI editor for annotations
    #[arg(short, long)]
    pub edit: bool,

    /// Apply rounded corners and soft drop shadow
    #[arg(long)]
    pub pretty: bool,

    /// Do not copy screenshot to clipboard
    #[arg(long)]
    pub no_copy: bool,

    /// Pre-selected rectangular capture geometry (X,Y WxH)
    #[arg(short, long)]
    pub geometry: Option<String>,
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

    /// Apply rounded corners and soft drop shadow
    #[arg(long)]
    pub pretty: bool,
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

    /// Apply rounded corners and soft drop shadow
    #[arg(long)]
    pub pretty: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct PinArgs {
    /// Path to image file to pin
    #[arg(value_name = "FILE")]
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
        let args = Cli::parse_from(["kshot", "gui", "-c", "-d", "100", "--pretty"]);
        if let Some(Commands::Gui(gui)) = args.command {
            assert!(gui.clipboard);
            assert_eq!(gui.delay, 100);
            assert!(gui.pretty);
        } else {
            panic!("Expected Gui command");
        }
    }

    #[test]
    fn test_cli_parsing_hud() {
        let args = Cli::parse_from(["kshot", "hud", "--pretty"]);
        if let Some(Commands::Hud(hud)) = args.command {
            assert!(hud.pretty);
        } else {
            panic!("Expected Hud command");
        }
    }

    #[test]
    fn test_cli_parsing_window() {
        let args = Cli::parse_from(["kshot", "window", "--pretty", "-e"]);
        if let Some(Commands::Window(win)) = args.command {
            assert!(win.pretty);
            assert!(win.edit);
            assert!(win.clipboard);
        } else {
            panic!("Expected Window command");
        }
    }

    #[test]
    fn test_cli_parsing_scroll() {
        let args = Cli::parse_from(["kshot", "scroll", "--no-copy", "-p", "/tmp/out.png"]);
        if let Some(Commands::Scroll(scroll)) = args.command {
            assert!(scroll.no_copy);
            assert_eq!(scroll.path, Some(PathBuf::from("/tmp/out.png")));
        } else {
            panic!("Expected Scroll command");
        }
    }
}
