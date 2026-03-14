use crate::engine::ScoringEngine;
use crate::repository::models::{LeadScore, RawLeadData};

pub struct WealthIntentScorer;

impl ScoringEngine for WealthIntentScorer {
    fn score(&self, lead: &RawLeadData) -> LeadScore {
        let mut score = 0;
        let mut signals = Vec::new();

        let high_intent_keywords = vec!["Probate", "Inheritance", "Trust", "Estate", "Succession"];

        for keyword in high_intent_keywords {
            let full_name_contains = lead
                .full_name
                .to_lowercase()
                .contains(&keyword.to_lowercase());
            let signals_contains = lead
                .signals
                .iter()
                .any(|s| s.to_lowercase().contains(&keyword.to_lowercase()));

            if full_name_contains || signals_contains {
                score += 25;
                signals.push(format!("keyword_{}", keyword.to_lowercase()));
            }
        }

        LeadScore {
            score: score.min(100),
            signals,
        }
    }
}

pub struct ProfessionalReferralScorer;

impl ScoringEngine for ProfessionalReferralScorer {
    fn score(&self, lead: &RawLeadData) -> LeadScore {
        let mut score = 0;
        let mut signals = Vec::new();

        let prof_keywords = vec!["Attorney", "Lawyer", "CPA", "Accountant", "Firm"];

        for keyword in prof_keywords {
            if lead
                .full_name
                .to_lowercase()
                .contains(&keyword.to_lowercase())
            {
                score += 20;
                signals.push(format!("prof_{}", keyword.to_lowercase()));
            }
        }

        LeadScore {
            score: score.min(100),
            signals,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::models::RawLeadData;

    #[test]
    fn test_wealth_intent_scoring() {
        let scorer = WealthIntentScorer;
        let lead = RawLeadData {
            full_name: "John Probate Doe".to_string(),
            contact_info: serde_json::json!({}),
            source_url: "http://example.com".to_string(),
            signals: vec![],
        };

        let result = scorer.score(&lead);
        assert_eq!(result.score, 25);
        assert!(result.signals.contains(&"keyword_probate".to_string()));
    }
}
