use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Script {
    metadata: Metadata,
    source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    pub api: u32,
    pub name: String,
    pub description: String,
    pub author: String,
    pub icon: String,
    pub tags: String,
}

impl Script {
    pub fn from_source(source: String) -> Result<Self, String> {
        let start = source.find("/**").ok_or("No metadata")?;
        let end = source.find("**/").ok_or("No metadata")?;

        let metadata = serde_json::from_str(&source[start+3..end])
            .map_err(|e| format!("Could not pass metadata: {}", e));

        metadata.map(|m| Script {
            metadata: m,
            source,
        })
    }

    pub fn metadata(&self) -> &Metadata {
        return &self.metadata;
    }

    pub fn source(&self) -> &str {
        return &self.source;
    }
}
