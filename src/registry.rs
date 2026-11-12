//! A company's public record, from its country's official register
//! (decision 126): Companies House for the United Kingdom, with the key the
//! founder gives it, and the State's company directory for France
//! (recherche-entreprises.api.gouv.fr), which needs none. What it says fills
//! the new business's profile, for the founder to check before it is kept.
//! Only these two hosts are ever called, by Airth, never by an agent.

use crate::realm::{read_env_file, remove_env_entry, set_env_entry};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// Where the Companies House key is kept, in Airth's home.
const FILE: &str = "registries.env";
const GB_KEY: &str = "COMPANIES_HOUSE_KEY";
const GB_API: &str = "https://api.company-information.service.gov.uk";
const FR_API: &str = "https://recherche-entreprises.api.gouv.fr";

/// What the register says, in the profile's own fields (decision 52). A
/// field the register does not give is None.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Company {
    pub name: String,
    pub legal_name: String,
    pub legal_form: Option<String>,
    pub country: String,
    pub registration_number: String,
    pub registered_office: Option<String>,
    pub incorporated_on: Option<String>,
    pub fiscal_year_end: Option<String>,
    pub vat_number: Option<String>,
    pub currency: String,
    /// Still trading, as the register says.
    pub active: bool,
    /// What it does, as the register words it (a SIC or NAF code).
    pub activity: Option<String>,
}

/// Whether the founder gave a Companies House key.
pub fn has_gb_key(home: &Path) -> bool {
    gb_key(home).is_some()
}

fn gb_key(home: &Path) -> Option<String> {
    read_env_file(&home.join(FILE))
        .ok()?
        .into_iter()
        .find(|(k, _)| k == GB_KEY)
        .map(|(_, v)| v)
        .filter(|v| !v.trim().is_empty())
}

/// Keep the founder's Companies House key; an empty one forgets it.
pub fn set_gb_key(home: &Path, key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        remove_env_entry(&home.join(FILE), GB_KEY)?;
        return Ok(());
    }
    if key.len() > 100 || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        bail!("this is not a Companies House API key");
    }
    set_env_entry(&home.join(FILE), GB_KEY, key)
}

/// The company numbered `number` in `country`'s register.
pub fn lookup(home: &Path, country: &str, number: &str) -> Result<Company> {
    let number: String = number.chars().filter(|c| !c.is_whitespace()).collect();
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    match country {
        "GB" => {
            let number = gb_number(&number)?;
            let key = gb_key(home).context(
                "Companies House needs a key: create one, free, at developer.company-information.service.gov.uk",
            )?;
            let response = client
                .get(format!("{GB_API}/company/{number}"))
                .basic_auth(key, Some(""))
                .send()
                .context("Companies House did not answer")?;
            match response.status().as_u16() {
                200 => Ok(from_gb(&response.json()?)),
                401 => bail!("Companies House refused the key: check it"),
                404 => bail!("Companies House has no company {number}"),
                code => bail!("Companies House answered HTTP {code}"),
            }
        }
        "FR" => {
            let siren = fr_siren(&number)?;
            let response = client
                // Nine digits, checked: nothing to escape.
                .get(format!("{FR_API}/search?q={siren}&page=1&per_page=1"))
                .send()
                .context("the company directory did not answer")?;
            if !response.status().is_success() {
                bail!(
                    "the company directory answered HTTP {}",
                    response.status().as_u16()
                );
            }
            let found: Value = response.json()?;
            let first = found["results"]
                .as_array()
                .and_then(|r| r.iter().find(|c| c["siren"] == siren.as_str()))
                .with_context(|| format!("no French company has the SIREN {siren}"))?;
            Ok(from_fr(first))
        }
        _ => bail!("Airth reads the registers of the United Kingdom and France only"),
    }
}

/// A Companies House number: eight characters, digits or a two-letter
/// prefix (SC, NI, OC…) and six digits; shorter numbers take leading zeros.
fn gb_number(number: &str) -> Result<String> {
    let n = number.to_uppercase();
    let ok = n.len() <= 8
        && !n.is_empty()
        && n.chars().all(|c| c.is_ascii_alphanumeric())
        && n.chars().skip(2).all(|c| c.is_ascii_digit());
    if !ok {
        bail!("a company number is 8 characters, like 16838911 or SC123456");
    }
    Ok(if n.chars().all(|c| c.is_ascii_digit()) {
        format!("{n:0>8}")
    } else {
        n
    })
}

/// A SIREN, from a SIREN or a SIRET (its first nine digits).
fn fr_siren(number: &str) -> Result<String> {
    if !(number.len() == 9 || number.len() == 14) || !number.chars().all(|c| c.is_ascii_digit()) {
        bail!("a SIREN is 9 digits (or a SIRET, 14)");
    }
    Ok(number[..9].to_string())
}

fn text(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A name as people write it: "HEDGEN LTD" is Hedgen.
fn plain(legal: &str) -> String {
    const ENDINGS: [&str; 6] = [" LIMITED", " LTD", " PLC", " LLP", " SAS", " SARL"];
    let mut name = legal.trim().to_string();
    for e in ENDINGS {
        if name.to_uppercase().ends_with(e) {
            name.truncate(name.len() - e.len());
        }
    }
    if name.chars().any(|c| c.is_lowercase()) {
        return name;
    }
    name.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase()
                    .chain(c.flat_map(char::to_lowercase))
                    .collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn from_gb(c: &Value) -> Company {
    let a = &c["registered_office_address"];
    let office: Vec<String> = [
        "premises",
        "address_line_1",
        "address_line_2",
        "locality",
        "region",
        "postal_code",
    ]
    .iter()
    .filter_map(|k| text(&a[*k]))
    .collect();
    let ard = &c["accounts"]["accounting_reference_date"];
    let day = |k: &str| {
        ard[k]
            .as_str()
            .and_then(|v| v.parse::<u32>().ok())
            .or_else(|| ard[k].as_u64().map(|v| v as u32))
    };
    let legal_name = text(&c["company_name"]).unwrap_or_default();
    Company {
        name: plain(&legal_name),
        legal_form: text(&c["type"]).map(|t| {
            match t.as_str() {
                "ltd" | "private-unlimited" | "private-limited-guarant-nsc" => "ltd",
                "plc" => "plc",
                "llp" => "llp",
                other => other,
            }
            .to_string()
        }),
        country: "GB".into(),
        registration_number: text(&c["company_number"]).unwrap_or_default(),
        registered_office: (!office.is_empty()).then(|| office.join(", ")),
        incorporated_on: text(&c["date_of_creation"]),
        fiscal_year_end: day("month")
            .zip(day("day"))
            .map(|(m, d)| format!("{m:02}-{d:02}")),
        vat_number: None,
        currency: "GBP".into(),
        active: c["company_status"] == "active",
        activity: c["sic_codes"]
            .as_array()
            .and_then(|s| s.first())
            .and_then(text)
            .map(|sic| format!("SIC {sic}")),
        legal_name,
    }
}

/// The legal forms the directory's codes name most often.
fn fr_form(code: &str) -> Option<&'static str> {
    Some(match code {
        "5710" => "sas",
        "5720" => "sasu",
        "5499" | "5498" => "sarl",
        "5599" | "5505" | "5510" | "5515" => "sa",
        "1000" => "ei",
        "6540" => "sci",
        _ => return None,
    })
}

fn from_fr(c: &Value) -> Company {
    let siren = text(&c["siren"]).unwrap_or_default();
    let legal_name = text(&c["nom_raison_sociale"])
        .or_else(|| text(&c["nom_complet"]))
        .unwrap_or_default();
    Company {
        name: plain(&legal_name),
        legal_form: text(&c["nature_juridique"]).and_then(|n| fr_form(&n).map(str::to_string)),
        country: "FR".into(),
        registered_office: text(&c["siege"]["adresse"]),
        incorporated_on: text(&c["date_creation"]),
        // The directory does not give the year end.
        fiscal_year_end: None,
        vat_number: fr_vat(&siren),
        currency: "EUR".into(),
        active: c["etat_administratif"] == "A",
        activity: text(&c["activite_principale"]).map(|naf| format!("NAF {naf}")),
        registration_number: siren,
        legal_name,
    }
}

/// The French intra-community VAT number a SIREN gives: FR, a two-digit
/// key, the SIREN.
fn fr_vat(siren: &str) -> Option<String> {
    let n: u64 = siren.parse().ok().filter(|_| siren.len() == 9)?;
    Some(format!("FR{:02}{siren}", (12 + 3 * (n % 97)) % 97))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_companies_house_record_fills_the_profile() {
        let c = from_gb(&json!({
            "company_name": "HEDGEN LTD",
            "company_number": "16838911",
            "company_status": "active",
            "type": "ltd",
            "date_of_creation": "2025-11-07",
            "registered_office_address": {
                "address_line_1": "71-75 Shelton Street",
                "locality": "London",
                "postal_code": "WC2H 9JQ",
                "premises": "Covent Garden"
            },
            "accounts": {"accounting_reference_date": {"day": "30", "month": "11"}},
            "sic_codes": ["58190"]
        }));
        assert_eq!(c.name, "Hedgen");
        assert_eq!(c.legal_name, "HEDGEN LTD");
        assert_eq!(c.legal_form.as_deref(), Some("ltd"));
        assert_eq!(c.fiscal_year_end.as_deref(), Some("11-30"));
        assert_eq!(c.incorporated_on.as_deref(), Some("2025-11-07"));
        assert_eq!(
            c.registered_office.as_deref(),
            Some("Covent Garden, 71-75 Shelton Street, London, WC2H 9JQ")
        );
        assert_eq!((c.currency.as_str(), c.active), ("GBP", true));
        assert_eq!(c.activity.as_deref(), Some("SIC 58190"));
    }

    #[test]
    fn a_french_record_fills_the_profile_and_its_vat_number() {
        let c = from_fr(&json!({
            "siren": "552032534",
            "nom_complet": "DANONE",
            "nom_raison_sociale": "DANONE",
            "nature_juridique": "5599",
            "date_creation": "1955-01-01",
            "etat_administratif": "A",
            "activite_principale": "70.10Z",
            "siege": {"adresse": "17 BOULEVARD HAUSSMANN 75009 PARIS 9"}
        }));
        assert_eq!(c.name, "Danone");
        assert_eq!(c.legal_form.as_deref(), Some("sa"));
        assert_eq!(c.vat_number.as_deref(), Some("FR27552032534"));
        assert_eq!(c.fiscal_year_end, None);
        assert_eq!((c.currency.as_str(), c.active), ("EUR", true));
    }

    #[test]
    fn numbers_are_checked_before_any_call() {
        assert_eq!(gb_number("1234567").unwrap(), "01234567");
        assert_eq!(gb_number("sc123456").unwrap(), "SC123456");
        assert!(gb_number("../etc").is_err());
        assert!(gb_number("123456789").is_err());
        assert_eq!(fr_siren("55203253400646").unwrap(), "552032534");
        assert!(fr_siren("55203253").is_err());
        let home = tempfile::tempdir().unwrap();
        let err = lookup(home.path(), "DE", "HRB 1").unwrap_err().to_string();
        assert!(err.contains("United Kingdom and France only"), "{err}");
        let err = lookup(home.path(), "GB", "16838911")
            .unwrap_err()
            .to_string();
        assert!(err.contains("needs a key"), "{err}");
    }

    #[test]
    fn the_key_is_kept_checked_and_forgotten() {
        let home = tempfile::tempdir().unwrap();
        assert!(!has_gb_key(home.path()));
        assert!(set_gb_key(home.path(), "a b\nc").is_err());
        set_gb_key(home.path(), " 1234-abcd ").unwrap();
        assert!(has_gb_key(home.path()));
        set_gb_key(home.path(), "").unwrap();
        assert!(!has_gb_key(home.path()));
    }
}
