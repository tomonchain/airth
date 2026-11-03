//! A post on X the founder sent to answer (decision 63): its link, read
//! through X's public oEmbed endpoint, and shaped like a feed item so the
//! check and the graph rule work unchanged. A post whose own words are too
//! thin to answer is refused, never drafted.

use crate::pipeline::run::Item;
use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Words of its own a post needs before a reply can be written from it.
const MIN_WORDS: usize = 8;

/// The first post link in `text`, as `https://x.com/<user>/status/<id>`:
/// x.com or twitter.com (www. or mobile. too), nothing else.
pub fn link_in(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|word| {
        let word = word.trim_matches(|c: char| "<>(),".contains(c));
        let rest = word
            .strip_prefix("https://")
            .or_else(|| word.strip_prefix("http://"))?;
        let (host, path) = rest.split_once('/')?;
        let host = host
            .trim_start_matches("www.")
            .trim_start_matches("mobile.");
        if host != "x.com" && host != "twitter.com" {
            return None;
        }
        let mut parts = path.split(['?', '#']).next()?.split('/');
        let (user, status, id) = (parts.next()?, parts.next()?, parts.next()?);
        let fits = !user.is_empty()
            && user.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && status == "status"
            && !id.is_empty()
            && id.chars().all(|c| c.is_ascii_digit());
        fits.then(|| format!("https://x.com/{user}/status/{id}"))
    })
}

/// What the founder wrote besides the link: the angle they want, if any.
pub fn angle_in(text: &str) -> String {
    text.split_whitespace()
        .filter(|w| link_in(w).is_none())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where X's oEmbed answers for a link from `link_in`.
pub fn oembed_url(link: &str) -> String {
    let encoded = link.replace(':', "%3A").replace('/', "%2F");
    format!("https://publish.x.com/oembed?omit_script=true&dnt=true&url={encoded}")
}

/// The post as an item, from oEmbed's answer `body`: its author and first
/// words as `title`, its text as `summary`, its date, its link. The
/// founder's `angle` goes beside it, never into the post's own fields.
pub fn item(body: &str, link: &str, angle: &str) -> Result<Item> {
    let answer: Value = serde_json::from_str(body)
        .context("X did not show this post: it may be protected or deleted")?;
    let html = answer["html"]
        .as_str()
        .context("X did not show this post: it may be protected or deleted")?;
    let author = answer["author_name"].as_str().unwrap_or_default();
    let words = html
        .split_once("<p")
        .and_then(|(_, rest)| rest.split_once('>'))
        .and_then(|(_, rest)| rest.split_once("</p>"))
        .map(|(text, _)| text)
        .unwrap_or_default();
    let text = plain(&words.replace("<br>", "\n"));
    // The date is the text of the blockquote's last link.
    let date = html
        .rsplit_once("</a>")
        .and_then(|(before, _)| before.rsplit_once('>'))
        .map(|(_, date)| plain(date))
        .unwrap_or_default();
    if let Some(why) = too_thin(&text) {
        bail!("not drafted: the post is {why}, too little to reply to");
    }
    let link = answer["url"]
        .as_str()
        .and_then(link_in)
        .unwrap_or_else(|| link.to_string());
    let first: String = text.chars().take(80).collect();
    let mut item = Item::new();
    let mut set = |k: &str, v: &str| {
        item.insert(k.into(), v.into());
    };
    set("title", &format!("{author}: {first}"));
    set("summary", &text);
    set("published", &date);
    set("link", &link);
    if !angle.trim().is_empty() {
        set("founder_angle", angle.trim());
    }
    Ok(item)
}

/// Why a post's own words are too thin to answer, if they are.
fn too_thin(text: &str) -> Option<&'static str> {
    let own = text
        .split_whitespace()
        .filter(|w| {
            !w.starts_with("http") && !w.starts_with("pic.twitter.com") && !w.starts_with('@')
        })
        .count();
    if own >= MIN_WORDS {
        return None;
    }
    Some(if text.contains("pic.twitter.com") {
        "mostly an image or a chart"
    } else if text.contains("http") {
        "mostly a link or a quoted post"
    } else {
        "only a few words"
    })
}

/// Text of an HTML fragment: tags out, the common entities decoded,
/// spaces joined, lines kept.
fn plain(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&mdash;", "—")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&");
    decoded
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_post_link_is_found_and_made_canonical() {
        for text in [
            "https://x.com/jack/status/20",
            "push back on this https://twitter.com/jack/status/20?s=46&t=abc",
            "<https://mobile.twitter.com/jack/status/20>",
            "https://www.x.com/jack/status/20#m",
        ] {
            assert_eq!(
                link_in(text).as_deref(),
                Some("https://x.com/jack/status/20"),
                "{text}"
            );
        }
        for text in [
            "https://x.com/jack",
            "https://evil.com/x.com/jack/status/20",
            "https://x.com.evil.com/jack/status/20",
            "https://x.com/jack/status/20abc",
            "https://x.com/../status/20",
            "no link here",
        ] {
            assert_eq!(link_in(text), None, "{text}");
        }
        assert_eq!(
            angle_in("https://x.com/jack/status/20 push back on the yield claim"),
            "push back on the yield claim"
        );
        assert_eq!(
            oembed_url("https://x.com/jack/status/20"),
            "https://publish.x.com/oembed?omit_script=true&dnt=true&url=https%3A%2F%2Fx.com%2Fjack%2Fstatus%2F20"
        );
    }

    fn answer(text: &str) -> String {
        serde_json::json!({
            "url": "https://x.com/jack/status/20",
            "author_name": "jack",
            "html": format!("<blockquote class=\"twitter-tweet\"><p lang=\"en\" dir=\"ltr\">{text}</p>&mdash; jack (@jack) <a href=\"https://x.com/jack/status/20?ref_src=twsrc%5Etfw\">March 21, 2006</a></blockquote>\n<script async></script>")
        })
        .to_string()
    }

    /// The post's own words become a feed-like item; the founder's angle
    /// stays apart, as their intent.
    #[test]
    fn a_post_reads_like_a_feed_item_with_the_angle_apart() {
        let body = answer(
            "Staking yields fell to 2.8% this week &amp; withdrawals doubled.<br>Not a run yet.",
        );
        let item = item(
            &body,
            "https://x.com/jack/status/20",
            "push back on the yield claim",
        )
        .unwrap();
        assert_eq!(
            item["summary"],
            "Staking yields fell to 2.8% this week & withdrawals doubled.\nNot a run yet."
        );
        assert!(
            item["title"]
                .as_str()
                .unwrap()
                .starts_with("jack: Staking yields fell")
        );
        assert_eq!(item["published"], "March 21, 2006");
        assert_eq!(item["link"], "https://x.com/jack/status/20");
        assert_eq!(item["founder_angle"], "push back on the yield claim");
        let bare = super::item(&body, "https://x.com/jack/status/20", " ").unwrap();
        assert!(!bare.contains_key("founder_angle"));
    }

    /// Decision 63: too thin to answer is said, never drafted.
    #[test]
    fn a_thin_post_is_refused_with_why() {
        let thin = |text: &str| format!("{:#}", item(&answer(text), "l", "").unwrap_err());
        assert!(thin("Look at this pic.twitter.com/abc").contains("mostly an image or a chart"));
        assert!(thin("This. https://t.co/xyz").contains("mostly a link or a quoted post"));
        assert!(thin("gm").contains("only a few words"));
        assert!(
            format!("{:#}", item("<html>", "l", "").unwrap_err()).contains("protected or deleted")
        );
    }
}
