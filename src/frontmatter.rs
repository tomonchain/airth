//! Markdown files with a flat YAML front matter (decision 43): structured
//! fields between `---` lines, prose after. Flat means `key: value`, one per
//! line, no nesting. A value is text (quoted or not, or folded with `>` or
//! `|` over indented lines) or an inline list (`[a, "b, c"]`). An empty value
//! or `TODO` is not known yet. What each kind of file may hold is checked by
//! its reader, which refuses keys it does not know.

use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    List(Vec<String>),
}

/// A parsed file: its fields in order, `None` where the value is not known
/// yet, and its body.
#[derive(Debug, Default, PartialEq)]
pub struct Doc {
    pub fields: Vec<(String, Option<Value>)>,
    pub body: String,
}

pub fn parse(text: &str) -> Doc {
    let text = text.trim_start_matches('\u{feff}');
    let Some(rest) = text.strip_prefix("---") else {
        return Doc {
            fields: Vec::new(),
            body: text.to_string(),
        };
    };
    let (block, body) = match rest.find("\n---") {
        Some(end) => {
            let after = &rest[end + 4..];
            (&rest[..end], after.split_once('\n').map_or("", |(_, b)| b))
        }
        None => (rest, ""),
    };
    let lines: Vec<&str> = block.lines().collect();
    let mut fields = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some((key, value)) = line.split_once(':')
            && !line.starts_with([' ', '\t', '#'])
        {
            let mut value = value.trim().to_string();
            if [">", "|", ">-", "|-"].contains(&value.as_str()) {
                let mut parts = Vec::new();
                while i + 1 < lines.len()
                    && (lines[i + 1].starts_with([' ', '\t']) || lines[i + 1].trim().is_empty())
                {
                    i += 1;
                    parts.push(lines[i].trim());
                }
                value = parts.join(" ").trim().to_string();
            }
            fields.push((key.trim().to_string(), value_of(&value)));
        }
        i += 1;
    }
    Doc {
        fields,
        body: body.to_string(),
    }
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|s| s.strip_suffix(q)) {
            return inner;
        }
    }
    s
}

fn value_of(raw: &str) -> Option<Value> {
    if let Some(inner) = raw.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        let mut items = Vec::new();
        let (mut item, mut quote) = (String::new(), None);
        for c in inner.chars() {
            match quote {
                Some(q) if c == q => quote = None,
                None if c == '"' || c == '\'' => quote = Some(c),
                None if c == ',' => {
                    items.push(std::mem::take(&mut item));
                    continue;
                }
                _ => {}
            }
            item.push(c);
        }
        items.push(item);
        let items: Vec<String> = items
            .iter()
            .map(|i| unquote(i).to_string())
            .filter(|i| !i.is_empty() && i != "TODO")
            .collect();
        return Some(Value::List(items));
    }
    let text = unquote(raw);
    (!text.is_empty() && text != "TODO").then(|| Value::Text(text.to_string()))
}

impl Doc {
    /// Refuse a key the file may not hold: a typo must not drop a field.
    pub fn only(&self, allowed: &[&str]) -> Result<()> {
        for (key, _) in &self.fields {
            if !allowed.contains(&key.as_str()) {
                bail!(
                    "unknown field '{key}' (expected one of: {})",
                    allowed.join(", ")
                );
            }
        }
        Ok(())
    }

    fn get(&self, key: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.as_ref())
    }

    /// A text field, when known.
    pub fn text(&self, key: &str) -> Option<String> {
        match self.get(key) {
            Some(Value::Text(t)) => Some(t.clone()),
            Some(Value::List(l)) => (!l.is_empty()).then(|| l.join(", ")),
            None => None,
        }
    }

    /// A list field; a single text is a list of one.
    pub fn list(&self, key: &str) -> Vec<String> {
        match self.get(key) {
            Some(Value::List(l)) => l.clone(),
            Some(Value::Text(t)) => vec![t.clone()],
            None => Vec::new(),
        }
    }

    /// Of `keys`, those not known yet.
    pub fn unknown(&self, keys: &[&str]) -> Vec<String> {
        keys.iter()
            .filter(|k| self.get(k).is_none_or(|v| v == &Value::List(Vec::new())))
            .map(|k| k.to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_flat_fields_lists_and_the_body() {
        let doc = parse(
            "---\nname: Hedgen\nlegal_name: \"Hedgen Ltd\"\nvat_number: TODO\ndomain:\n\
             links: [https://x.com/hedgen, \"a, b\", TODO]\ndescription: >\n  two\n  lines\n---\n# Hedgen\n\nProse.\n",
        );
        assert_eq!(doc.text("name").as_deref(), Some("Hedgen"));
        assert_eq!(doc.text("legal_name").as_deref(), Some("Hedgen Ltd"));
        assert_eq!(doc.text("vat_number"), None);
        assert_eq!(doc.text("domain"), None);
        assert_eq!(doc.list("links"), vec!["https://x.com/hedgen", "a, b"]);
        assert_eq!(doc.text("description").as_deref(), Some("two lines"));
        assert_eq!(doc.body, "# Hedgen\n\nProse.\n");
        assert_eq!(
            doc.unknown(&["name", "vat_number", "domain", "absent"]),
            vec!["vat_number", "domain", "absent"]
        );
        assert!(
            doc.only(&[
                "name",
                "legal_name",
                "vat_number",
                "domain",
                "links",
                "description"
            ])
            .is_ok()
        );
        let err = doc.only(&["name"]).unwrap_err().to_string();
        assert!(err.contains("unknown field 'legal_name'"), "{err}");
    }

    #[test]
    fn a_file_without_front_matter_is_all_body() {
        let doc = parse("# Hedgen\nNo fields.");
        assert!(doc.fields.is_empty());
        assert_eq!(doc.body, "# Hedgen\nNo fields.");
        assert_eq!(parse("---\nname: x\n").text("name").as_deref(), Some("x"));
    }
}
