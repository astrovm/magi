use crate::platform::Res;
use crate::text::escape_html;

pub const HTML_CONTENT_TYPE: &str = "text/html; charset=utf-8";

#[derive(Debug, Clone, Default)]
pub struct PageOptions<'a> {
    pub title: Option<&'a str>,
    pub status: Option<u16>,
    pub headers: Vec<(&'static str, String)>,
    /// One more script for the page, from the site's own files.
    pub script: Option<&'static str>,
}

pub fn render_page(body: &str, options: PageOptions<'_>) -> Res {
    let title = options.title.map_or_else(
        || "Magi".to_owned(),
        |title| format!("{} | Magi", escape_html(title)),
    );
    let script = options.script.map_or_else(String::new, |script| {
        format!(r#"<script src="{script}" defer></script>"#)
    });
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <meta name="robots" content="noindex" />
  <title>{title}</title>
  <link rel="icon" type="image/png" sizes="32x32" href="/favicon-32x32.png" />
  <link rel="preload" href="/fonts/fredoka-latin.woff2" as="font" type="font/woff2" crossorigin />
  <link rel="stylesheet" href="/style.css" />
  <script src="/app.js" defer></script>
  {script}
</head>
<body class="page">
  <div class="season" aria-hidden="true"></div>
  <main class="stage">
    <a href="/" class="logo-link"><img src="/logo3d.webp" alt="Magi" class="logo" /></a>
    <section class="card">{body}</section>
  </main>
</body>
</html>"#
    );

    let mut response = Res::new(options.status.unwrap_or(200))
        .with_header("Content-Type", HTML_CONTENT_TYPE)
        .with_body(html);
    response.headers.extend(options.headers);
    response
}
