//! Model prices, to estimate what model calls cost. Estimates from public
//! list prices, labelled as such wherever they are shown; the owner adds or
//! replaces prices in `prices.json` in the Airth home.

use crate::home::Home;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

impl Price {
    /// Cache reads at a tenth of the input price, 5-minute cache writes at
    /// 1.25 times, as Anthropic bills them.
    const fn standard(input: f64, output: f64) -> Self {
        Price {
            input,
            output,
            cache_read: input / 10.0,
            cache_write: input * 1.25,
        }
    }
}

/// List prices of Anthropic's first-party API (October 2026), by model id
/// prefix. Other providers have none: their costs show as unknown unless
/// the owner adds prices in `prices.json` in the Airth home.
const LIST_PRICES: &[(&str, Price)] = &[
    (
        "claude-fable-5-1",
        Price {
            cache_read: 0.25,
            ..Price::standard(10.0, 50.0)
        },
    ),
    ("claude-fable-5", Price::standard(10.0, 50.0)),
    (
        "claude-opus-5-5",
        Price {
            cache_read: 0.20,
            ..Price::standard(4.0, 20.0)
        },
    ),
    ("claude-opus-5", Price::standard(5.0, 25.0)),
    ("claude-opus-4-8", Price::standard(5.0, 25.0)),
    ("claude-opus-4-7", Price::standard(5.0, 25.0)),
    ("claude-opus-4-6", Price::standard(5.0, 25.0)),
    ("claude-sonnet-5-5", Price::standard(2.0, 10.0)),
    ("claude-sonnet-5", Price::standard(2.0, 10.0)),
    ("claude-sonnet-4-6", Price::standard(3.0, 15.0)),
    ("claude-haiku-5-5", Price::standard(0.10, 0.50)),
    ("claude-haiku-4-5", Price::standard(1.0, 5.0)),
];

/// Prices by model id prefix, the longest prefix winning.
#[derive(Debug, Clone)]
pub struct Prices(Vec<(String, Price)>);

impl Default for Prices {
    fn default() -> Self {
        Self::with(BTreeMap::new())
    }
}

impl Prices {
    /// The list prices, with `overrides` added (or replacing them).
    pub fn with(overrides: BTreeMap<String, Price>) -> Self {
        let mut all: Vec<(String, Price)> = LIST_PRICES
            .iter()
            .filter(|(p, _)| !overrides.contains_key(*p))
            .map(|(p, price)| (p.to_string(), *price))
            .collect();
        all.extend(overrides);
        all.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
        Self(all)
    }

    pub fn get(&self, model: &str) -> Option<Price> {
        // Without the provider: "anthropic/claude-…", Bedrock's "anthropic.claude-…".
        let id = model.rsplit('/').next().unwrap_or(model);
        let id = id
            .split_once('.')
            .filter(|(_, rest)| rest.starts_with("claude"))
            .map_or(id, |(_, rest)| rest);
        self.0
            .iter()
            .find(|(p, _)| id.starts_with(p.as_str()))
            .map(|(_, price)| *price)
    }
}

impl Prices {
    /// The list prices, with the owner's own from `prices.json`.
    pub fn load(home: &Home) -> Self {
        let path = home.dir().join("prices.json");
        let overrides: BTreeMap<String, Price> = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                eprintln!("warning: ignoring {}: {e}", path.display());
                BTreeMap::new()
            }),
            Err(_) => BTreeMap::new(),
        };
        Self::with(overrides)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_price_by_the_longest_model_prefix() {
        let prices = Prices::default();
        assert_eq!(prices.get("claude-sonnet-5-5").unwrap().input, 2.0);
        // Provider names are ignored, the longest prefix wins.
        assert_eq!(
            prices.get("anthropic/claude-fable-5-1").unwrap().cache_read,
            0.25
        );
        assert_eq!(prices.get("claude-fable-5").unwrap().cache_read, 1.0);
        assert_eq!(
            prices.get("anthropic.claude-opus-5-5").unwrap().cache_read,
            0.20
        );
        // Unknown models have no price, unless the owner adds one.
        assert!(prices.get("openai/gpt-x").is_none());
        let custom = Prices::with(BTreeMap::from([(
            "gpt-x".to_string(),
            Price::standard(1.0, 1.0),
        )]));
        assert!(custom.get("openai/gpt-x").is_some());
    }

    #[test]
    fn reads_the_owners_prices_and_ignores_a_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::at(dir.path()).unwrap();
        std::fs::write(
            dir.path().join("prices.json"),
            r#"{"gpt-x": {"input": 1.0, "output": 2.0, "cache_read": 0.1, "cache_write": 1.0}}"#,
        )
        .unwrap();
        assert_eq!(Prices::load(&home).get("gpt-x").unwrap().output, 2.0);
        std::fs::write(dir.path().join("prices.json"), "not json").unwrap();
        let prices = Prices::load(&home);
        assert!(prices.get("gpt-x").is_none());
        assert!(prices.get("claude-opus-5").is_some());
    }
}
