//! Items from an RSS 2.0 or Atom feed. Feeds are external content: read as
//! data, never trusted to be well formed. A feed that breaks off (cut by
//! the size limit, or malformed) gives the items read before the break.

use chrono::{DateTime, Utc};
use quick_xml::Reader;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use serde_json::{Map, Value};

/// Summaries are cut here: enough to judge an item, not a whole article.
const MAX_SUMMARY: usize = 600;

pub struct Feed {
    /// Each item: title, link, id, published (as the feed wrote it), summary.
    pub items: Vec<Map<String, Value>>,
    /// False when the feed broke off before its end.
    pub complete: bool,
}

/// When an item was published, from the date its feed wrote: RSS writes
/// RFC 2822, Atom RFC 3339. None when it has none, or one that does not read.
pub fn published(item: &Map<String, Value>) -> Option<DateTime<Utc>> {
    let text = item.get("published")?.as_str()?.trim();
    DateTime::parse_from_rfc2822(text)
        .or_else(|_| DateTime::parse_from_rfc3339(text))
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

pub fn parse(xml: &str) -> Feed {
    let mut reader = Reader::from_str(xml);
    let mut items = Vec::new();
    // The item being read, how deep inside it we are, and the child whose text we collect.
    let mut item: Option<Map<String, Value>> = None;
    let mut depth = 0usize;
    let mut field: Option<String> = None;
    let mut text = String::new();
    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(_) => {
                return Feed {
                    items,
                    complete: false,
                };
            }
        };
        match event {
            Event::Eof => break,
            Event::Start(e) => {
                let name = local(&e);
                let Some(it) = item.as_mut() else {
                    if name == "item" || name == "entry" {
                        item = Some(Map::new());
                        depth = 0;
                    }
                    continue;
                };
                depth += 1;
                if depth == 1 {
                    if name == "link" {
                        atom_link(&e, it);
                    }
                    field = Some(name);
                    text.clear();
                }
            }
            Event::Empty(e) => {
                if let Some(it) = item.as_mut()
                    && depth == 0
                    && local(&e) == "link"
                {
                    atom_link(&e, it);
                }
            }
            Event::Text(t) if field.is_some() => text.push_str(&t.xml10_content()),
            Event::CData(t) if field.is_some() => text.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if field.is_some() => match r.resolve_char_ref() {
                Ok(Some(c)) => text.push(c),
                _ => {
                    let name = r.xml10_content();
                    match resolve_predefined_entity(&name) {
                        Some(s) => text.push_str(s),
                        None => text.push_str(&format!("&{name};")),
                    }
                }
            },
            Event::End(_) => {
                let Some(it) = item.as_mut() else { continue };
                if depth == 0 {
                    if let Some(done) = finish(item.take().unwrap_or_default()) {
                        items.push(done);
                    }
                    continue;
                }
                depth -= 1;
                if depth == 0
                    && let Some(name) = field.take()
                {
                    set(it, &name, text.trim());
                }
            }
            _ => {}
        }
    }
    Feed {
        items,
        complete: true,
    }
}

fn local(e: &BytesStart) -> String {
    e.local_name().into_inner().to_string()
}

/// Atom's `<link href="…">`: the page itself, not its other relations.
fn atom_link(e: &BytesStart, item: &mut Map<String, Value>) {
    let attr = |key: &str| {
        e.try_get_attribute(key).ok().flatten().and_then(|a| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned())
        })
    };
    if let Some(href) = attr("href")
        && attr("rel").is_none_or(|rel| rel == "alternate")
        && !item.contains_key("link")
    {
        item.insert("link".into(), href.into());
    }
}

fn set(item: &mut Map<String, Value>, name: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    let key = match name {
        "title" => "title",
        "link" => "link",
        "guid" | "id" => "id",
        // An entry's first publication wins over its last update.
        "published" | "pubDate" => {
            item.insert("published".into(), text.into());
            return;
        }
        "updated" | "date" => "published",
        "description" | "summary" | "content" => "summary",
        _ => return,
    };
    if !item.contains_key(key) {
        item.insert(key.into(), text.into());
    }
}

/// An item needs a title or a link; its id defaults to its link, and its
/// summary is plain text.
fn finish(mut item: Map<String, Value>) -> Option<Map<String, Value>> {
    if !item.contains_key("title") && !item.contains_key("link") {
        return None;
    }
    if !item.contains_key("id")
        && let Some(link) = item.get("link").cloned()
    {
        item.insert("id".into(), link);
    }
    if let Some(Value::String(summary)) = item.get("summary") {
        let plain = plain_text(summary);
        item.insert("summary".into(), plain.into());
    }
    Some(item)
}

/// Summaries are often HTML: keep the words.
fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let words = out.replace("&nbsp;", " ");
    let mut plain = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if plain.len() > MAX_SUMMARY {
        let mut cut = MAX_SUMMARY;
        while !plain.is_char_boundary(cut) {
            cut -= 1;
        }
        plain.truncate(cut);
        plain.push('…');
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(item: &'a Map<String, Value>, key: &str) -> &'a str {
        item.get(key).and_then(|v| v.as_str()).unwrap_or_default()
    }

    #[test]
    fn reads_rss_items() {
        let feed = parse(
            r#"<?xml version="1.0"?>
            <rss version="2.0"><channel><title>The blog</title>
              <item>
                <title>Flows &amp; yields</title>
                <link>https://example.com/a</link>
                <pubDate>Fri, 30 Oct 2026 07:00:00 GMT</pubDate>
                <description>&lt;p&gt;ETH left&amp;nbsp;the queue.&lt;/p&gt;</description>
              </item>
              <item><title><![CDATA[Second <b>one</b>]]></title><guid>tag:2</guid></item>
              <item><description>no title, no link: skipped</description></item>
            </channel></rss>"#,
        );
        assert!(feed.complete);
        assert_eq!(feed.items.len(), 2);
        let a = &feed.items[0];
        assert_eq!(field(a, "title"), "Flows & yields");
        assert_eq!(field(a, "link"), "https://example.com/a");
        assert_eq!(field(a, "id"), "https://example.com/a");
        assert_eq!(field(a, "published"), "Fri, 30 Oct 2026 07:00:00 GMT");
        assert_eq!(
            published(a).unwrap().to_rfc3339(),
            "2026-10-30T07:00:00+00:00"
        );
        assert_eq!(published(&feed.items[1]), None);
        assert_eq!(field(a, "summary"), "ETH left the queue.");
        assert_eq!(field(&feed.items[1], "title"), "Second <b>one</b>");
        assert_eq!(field(&feed.items[1], "id"), "tag:2");
    }

    #[test]
    fn reads_atom_entries() {
        let feed = parse(
            r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Feed</title>
              <entry>
                <title type="html">Staking &#8212; a note</title>
                <link rel="replies" href="https://example.com/a#comments"/>
                <link href="https://example.com/a"/>
                <id>urn:a</id>
                <updated>2026-10-30T08:00:00Z</updated>
                <published>2026-10-29T08:00:00Z</published>
                <summary>Short.</summary>
              </entry>
            </feed>"#,
        );
        assert!(feed.complete);
        let a = &feed.items[0];
        assert_eq!(field(a, "title"), "Staking — a note");
        assert_eq!(field(a, "link"), "https://example.com/a");
        assert_eq!(field(a, "id"), "urn:a");
        assert_eq!(field(a, "published"), "2026-10-29T08:00:00Z");
        assert_eq!(
            published(a).unwrap().to_rfc3339(),
            "2026-10-29T08:00:00+00:00"
        );
        let mut odd = a.clone();
        odd.insert("published".into(), "last Tuesday".into());
        assert_eq!(published(&odd), None);
        assert_eq!(field(a, "summary"), "Short.");
    }

    #[test]
    fn a_feed_that_breaks_off_keeps_what_came_before() {
        let feed = parse(
            "<rss><channel><item><title>Whole</title><link>https://e.com/1</link></item>\
             <item><title>Cut</title><link>https://e.com/2",
        );
        assert_eq!(feed.items.len(), 1);
        assert_eq!(field(&feed.items[0], "title"), "Whole");
        let broken = parse("<rss><channel><item><title>A</title></wrong></channel></rss>");
        assert!(!broken.complete);
        assert!(parse("not xml at all").items.is_empty());
    }

    #[test]
    fn long_summaries_are_cut() {
        let long = "word ".repeat(400);
        let feed = parse(&format!(
            "<rss><item><title>t</title><description>{long}</description></item></rss>"
        ));
        let summary = field(&feed.items[0], "summary");
        assert!(summary.len() <= MAX_SUMMARY + '…'.len_utf8());
        assert!(summary.ends_with('…'));
    }
}
