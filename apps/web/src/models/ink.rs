use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InkDocument {
    pub version: u32,
    pub width: f64,
    pub height: f64,
    #[serde(default = "default_ink_name")]
    pub name: String,
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default = "default_true")]
    pub strokes_on_top: bool,
    #[serde(default)]
    pub strokes: Vec<InkStroke>,
    #[serde(default)]
    pub embeds: Vec<InkEmbed>,
    #[serde(default)]
    pub thumbnail_data_url: Option<String>,
}

impl InkDocument {
    pub fn blank(width: f64, height: f64) -> Self {
        Self {
            version: 1,
            width,
            height,
            name: default_ink_name(),
            background: default_background(),
            strokes_on_top: true,
            strokes: Vec::new(),
            embeds: Vec::new(),
            thumbnail_data_url: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InkEmbed {
    pub id: String,
    pub kind: InkEmbedKind,
    pub src: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub z_index: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InkEmbedKind {
    Image,
    Video,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InkStroke {
    pub id: String,
    pub tool: InkTool,
    pub color: String,
    pub width: f64,
    pub opacity: f64,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub points: Vec<InkPoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InkTool {
    Pen,
    Highlighter,
    Eraser,
    Line,
    Rectangle,
    Circle,
    Select,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct InkPoint {
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_pressure")]
    pub pressure: f64,
}

fn default_pressure() -> f64 {
    1.0
}

fn default_true() -> bool {
    true
}

fn default_background() -> String {
    "#0b1020".to_string()
}

fn default_ink_name() -> String {
    "Whiteboard".to_string()
}
