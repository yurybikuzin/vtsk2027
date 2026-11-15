use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub series: Vec<Series>,
}

#[derive(Debug, Deserialize)]
pub struct Series {
    pub series_id: String,
    pub path: String,
    pub sheet: String,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub row_label: Option<String>,
    #[serde(default)]
    pub column: Option<String>,
    #[serde(default)]
    pub unit: String,
    #[serde(default = "default_period_style")]
    pub period_style: String,
    #[serde(default)]
    pub period_pattern: Option<String>,
    #[serde(default)]
    pub min_period: Option<String>,
    #[serde(default)]
    pub max_period: Option<String>,
}

fn default_period_style() -> String {
    "quarter_short".to_string()
}
