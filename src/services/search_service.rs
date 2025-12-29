use anyhow::Result;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Default, Clone)]
pub struct SearchService;

impl SearchService {
    pub async fn search(&self, _query: &str) -> Result<Vec<SearchResult>> {
        Ok(Vec::new())
    }
}
