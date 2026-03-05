use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InkDocument {
    pub version: u32,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub strokes: Vec<InkStroke>,
    #[serde(default)]
    pub thumbnail_data_url: Option<String>,
}

impl InkDocument {
    pub fn blank(width: f64, height: f64) -> Self {
        Self {
            version: 1,
            width,
            height,
            strokes: Vec::new(),
            thumbnail_data_url: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InkStroke {
    pub id: String,
    pub tool: InkTool,
    pub color: String,
    pub width: f64,
    pub opacity: f64,
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
    Lasso,
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
