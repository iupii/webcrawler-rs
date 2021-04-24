use std::marker::{Send, Sync};

pub trait HTTPClient: Send + Sync {
    fn fetch(&self, url: &str) -> Result<String, String>;
}

pub struct Reqwest;

impl HTTPClient for Reqwest {
    fn fetch(&self, url: &str) -> Result<String, String> {
        let body = reqwest::blocking::get(url).and_then(|v| v.text());
        match body {
            Ok(v) => Ok(v),
            Err(e) => Err(format!("{}", e))
        }
    }
}
