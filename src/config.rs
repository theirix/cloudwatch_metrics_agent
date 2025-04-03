use std::collections::HashMap;

#[derive(Debug)]
pub struct CloudwatchConfig {
    pub namespace: String,
    pub service_name: String,
    pub tags: HashMap<String, String>,
}
