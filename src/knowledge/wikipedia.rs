use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiRecord {
    pub title: String,
    pub summary: String,
    pub image_url: Option<String>,
    pub description: Option<String>,
}
