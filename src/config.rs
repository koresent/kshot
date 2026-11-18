use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub shortcuts: ShortcutsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            theme: ThemeConfig::default(),
            tools: ToolsConfig::default(),
            shortcuts: ShortcutsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_save_path")]
    pub save_path: String,
    #[serde(default = "default_filename_pattern")]
    pub filename_pattern: String,
    #[serde(default = "default_true")]
    pub auto_copy_on_save: bool,
    #[serde(default = "default_true")]
    pub close_after_action: bool,
    #[serde(default = "default_true")]
    pub show_notifications: bool,
    #[serde(default = "default_dim_opacity")]
    pub dim_opacity: u8,
    #[serde(default = "default_true")]
    pub magnifier_enabled: bool,
    #[serde(default = "default_magnifier_zoom")]
    pub magnifier_zoom: u32,
    #[serde(default = "default_magnifier_size")]
    pub magnifier_size: u32,
}

fn default_save_path() -> String {
    "~/Pictures/Screenshots".to_string()
}
fn default_filename_pattern() -> String {
    "Screenshot_%Y-%m-%d_%H-%M-%S.png".to_string()
}
fn default_true() -> bool {
    true
}
fn default_dim_opacity() -> u8 {
    175
}
fn default_magnifier_zoom() -> u32 {
    8
}
fn default_magnifier_size() -> u32 {
    11
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            save_path: default_save_path(),
            filename_pattern: default_filename_pattern(),
            auto_copy_on_save: true,
            close_after_action: true,
            show_notifications: true,
            dim_opacity: default_dim_opacity(),
            magnifier_enabled: true,
            magnifier_zoom: default_magnifier_zoom(),
            magnifier_size: default_magnifier_size(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default = "default_foreground")]
    pub foreground: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default = "default_selection_border")]
    pub selection_border: String,
    #[serde(default = "default_handle_color")]
    pub handle_color: String,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_draw_color")]
    pub default_color: String,
    #[serde(default = "default_thickness")]
    pub default_thickness: u32,
    #[serde(default = "default_swatches")]
    pub swatches: Vec<String>,
}

fn default_background() -> String {
    "#282828".to_string()
}
fn default_foreground() -> String {
    "#ebdbb2".to_string()
}
fn default_accent() -> String {
    "#fe8019".to_string()
}
fn default_selection_border() -> String {
    "#fabd2f".to_string()
}
fn default_handle_color() -> String {
    "#fe8019".to_string()
}
fn default_font_family() -> String {
    "JetBrainsMono Nerd Font".to_string()
}
fn default_draw_color() -> String {
    "#cc241d".to_string()
}
fn default_thickness() -> u32 {
    3
}
fn default_swatches() -> Vec<String> {
    vec![
        "#cc241d".to_string(), // Red
        "#98971a".to_string(), // Green
        "#d79921".to_string(), // Yellow
        "#458588".to_string(), // Blue
        "#b16286".to_string(), // Purple
        "#689d6a".to_string(), // Aqua
        "#fe8019".to_string(), // Orange
        "#ebdbb2".to_string(), // Light Foreground
        "#282828".to_string(), // Dark Background
        "#928374".to_string(), // Gray
    ]
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            background: default_background(),
            foreground: default_foreground(),
            accent: default_accent(),
            selection_border: default_selection_border(),
            handle_color: default_handle_color(),
            font_family: default_font_family(),
            default_color: default_draw_color(),
            default_thickness: default_thickness(),
            swatches: default_swatches(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "default_initial_tool")]
    pub initial_tool: String,
    #[serde(default = "default_arrow_style")]
    pub arrow_style: String,
    #[serde(default = "default_counter_start")]
    pub counter_start: u32,
    #[serde(default = "default_pixelate_block_size")]
    pub pixelate_block_size: u32,
    #[serde(default = "default_blur_radius")]
    pub blur_radius: u32,
}

fn default_initial_tool() -> String {
    "selection".to_string()
}
fn default_arrow_style() -> String {
    "curved".to_string()
}
fn default_counter_start() -> u32 {
    1
}
fn default_pixelate_block_size() -> u32 {
    10
}
fn default_blur_radius() -> u32 {
    8
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            initial_tool: default_initial_tool(),
            arrow_style: default_arrow_style(),
            counter_start: default_counter_start(),
            pixelate_block_size: default_pixelate_block_size(),
            blur_radius: default_blur_radius(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutsConfig {
    #[serde(default = "default_shortcuts")]
    pub bindings: HashMap<String, String>,
}

fn default_shortcuts() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("selection".to_string(), "s".to_string());
    m.insert("arrow".to_string(), "a".to_string());
    m.insert("rectangle".to_string(), "r".to_string());
    m.insert("circle".to_string(), "c".to_string());
    m.insert("pencil".to_string(), "p".to_string());
    m.insert("marker".to_string(), "m".to_string());
    m.insert("text".to_string(), "t".to_string());
    m.insert("counter".to_string(), "n".to_string());
    m.insert("pixelate".to_string(), "b".to_string());
    m.insert("invert".to_string(), "i".to_string());
    m.insert("copy".to_string(), "ctrl+c".to_string());
    m.insert("save".to_string(), "ctrl+s".to_string());
    m.insert("pin".to_string(), "space".to_string());
    m.insert("undo".to_string(), "ctrl+z".to_string());
    m.insert("redo".to_string(), "ctrl+shift+z".to_string());
    m.insert("select_all".to_string(), "ctrl+a".to_string());
    m.insert("cancel".to_string(), "escape".to_string());
    m.insert("confirm".to_string(), "enter".to_string());
    m.insert("toggle_help".to_string(), "question".to_string());
    m
}

impl Default for ShortcutsConfig {
    fn default() -> Self {
        Self {
            bindings: default_shortcuts(),
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("kshot").join("config.yaml")
        } else {
            PathBuf::from("config.yaml")
        }
    }

    pub fn load_or_create() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_yaml::from_str::<Config>(&content) {
                    Ok(cfg) => return cfg,
                    Err(e) => {
                        eprintln!("Warning: Failed to parse config file {:?}: {e}. Using defaults.", path);
                    }
                },
                Err(e) => {
                    eprintln!("Warning: Failed to read config file {:?}: {e}. Using defaults.", path);
                }
            }
        } else {
            let default_cfg = Config::default();
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(yaml) = serde_yaml::to_string(&default_cfg) {
                let header = "# kshot configuration — native Wayland screenshot & annotation tool\n# Generated automatically with Gruvbox Dark defaults\n\n";
                let full = format!("{}{}", header, yaml);
                let _ = fs::write(&path, full);
            }
            return default_cfg;
        }

        Config::default()
    }

    pub fn expanded_save_path(&self) -> PathBuf {
        let p = &self.general.save_path;
        if p.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(&p[2..]);
            }
        }
        PathBuf::from(p)
    }
}

pub fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8, u8)> {
    let s = hex.trim().trim_start_matches('#');
    match s.len() {
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some((r, g, b, 255))
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            let a = u8::from_str_radix(&s[6..8], 16).ok()?;
            Some((r, g, b, a))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_yaml_serialization() {
        let cfg = Config::default();
        let yaml = serde_yaml::to_string(&cfg).expect("Serialization failed");
        assert!(yaml.contains("save_path:"));
        assert!(yaml.contains("selection_border:"));
        assert!(yaml.contains("swatches:"));

        let deserialized: Config = serde_yaml::from_str(&yaml).expect("Deserialization failed");
        assert_eq!(deserialized.general.dim_opacity, 175);
        assert_eq!(deserialized.theme.accent, "#fe8019");
    }

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#cc241d"), Some((204, 36, 29, 255)));
        assert_eq!(parse_hex_color("#fe801980"), Some((254, 128, 25, 128)));
        assert_eq!(parse_hex_color("invalid"), None);
    }
}
