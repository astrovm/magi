const HTML_ESCAPES: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
};

const escapeHtml = (text: string): string => text.replace(/[&<>"']/g, (char) => HTML_ESCAPES[char]);

type PageOptions = {
  title?: string;
  status?: number;
  headers?: Record<string, string>;
};

const renderPage = (body: string, options: PageOptions = {}): Response => {
  const title = options.title ? `${escapeHtml(options.title)} | Magi` : 'Magi';
  const html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <meta name="robots" content="noindex" />
  <title>${title}</title>
  <link rel="icon" type="image/png" sizes="32x32" href="/favicon-32x32.png" />
  <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Fredoka:wght@400;600;700&display=swap" />
  <link rel="stylesheet" href="/style.css" />
  <script src="/app.js" defer></script>
</head>
<body class="page">
  <main class="stage">
    <a href="/" class="logo-link"><img src="/logo3d.webp" alt="Magi" class="logo" /></a>
    <section class="card">${body}</section>
  </main>
</body>
</html>`;

  return new Response(html, {
    status: options.status ?? 200,
    headers: { 'Content-Type': 'text/html; charset=utf-8', ...options.headers },
  });
};

export { escapeHtml, renderPage };
export type { PageOptions };
