use crate::engine::ExtractionEngine;
use crate::repository::models::RawLeadData;
use regex::Regex;
use scraper::{Html, Selector};

pub struct RegexExtractor;

impl ExtractionEngine for RegexExtractor {
    fn extract(&self, html: &str, url: &str) -> Vec<RawLeadData> {
        let mut leads = Vec::new();

        let email_re = Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();

        for m in email_re.find_iter(html) {
            leads.push(RawLeadData {
                full_name: "Unknown (Regex)".to_string(),
                contact_info: serde_json::json!({ "email": m.as_str() }),
                source_url: url.to_string(),
                signals: vec!["regex_email_match".to_string()],
            });
        }

        leads
    }
}

pub struct SelectorExtractor {
    pub name_selector: String,
    pub contact_selector: String,
}

impl ExtractionEngine for SelectorExtractor {
    fn extract(&self, html: &str, url: &str) -> Vec<RawLeadData> {
        let document = Html::parse_document(html);
        let name_sel = Selector::parse(&self.name_selector).unwrap();
        let contact_sel = Selector::parse(&self.contact_selector).unwrap();

        let mut leads = Vec::new();

        for name_element in document.select(&name_sel) {
            let name = name_element
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();
            let mut contact_info = serde_json::json!({});

            if let Some(contact_element) = document.select(&contact_sel).next() {
                contact_info = serde_json::json!({ "info": contact_element.text().collect::<Vec<_>>().join(" ").trim() });
            }

            leads.push(RawLeadData {
                full_name: name,
                contact_info,
                source_url: url.to_string(),
                signals: vec!["selector_match".to_string()],
            });
        }

        leads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_extraction() {
        let extractor = RegexExtractor;
        let html = "Contact us: john@example.com";
        let leads = extractor.extract(html, "http://test.com");

        assert_eq!(leads.len(), 1);
        assert_eq!(leads[0].contact_info["email"], "john@example.com");
    }
}
