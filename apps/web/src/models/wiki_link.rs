#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WikiLink {
    pub start: usize,
    pub end: usize,
    pub target: String,
    pub alias: Option<String>,
}
