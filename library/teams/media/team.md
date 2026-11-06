---
title: Media
description: A small newsroom for a media on social networks. It watches your sources each morning, drafts posts and replies for you to approve, writes a weekly article, and reports every Monday.
skills: []
connectors: []
---

- Write your business profile: what you publish, for whom, your editorial pillars and your voice.
- Put your news sources in routines/daily-watch.json and routines/daily-posts.json (their RSS feeds).
- Write your pillars and your voice in prompts/watch.md, prompts/pick.md, prompts/post.md and prompts/reply.md, where the square brackets are.
- Connect Typefully, where approved posts go as drafts. You publish them; Airth never posts.
- Give daily-watch and daily-posts a schedule (for example daily 07:00 and daily 07:30).
- Replies: declare typefully.create_reply (a draft with reply_to_url, never publish_at), then send a post's link to the bot on Telegram.
- The weekly article: declare your site's API as an integration named site, with an action publish_article that takes title, excerpt and content; give weekly-article a schedule (for example fri 09:00).
