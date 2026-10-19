/** The page Polar sends a buyer to after payment. Static: no auth, no wallet, no D1. */
const PAGE = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>Payment done</title>
<style>
:root{color-scheme:light dark;--bg:#f6f7f9;--fg:#14171c;--muted:#4b5563;--card:#fff;--line:#d9dde3}
@media (prefers-color-scheme:dark){:root{--bg:#0f1115;--fg:#eef0f3;--muted:#a4abb7;--card:#181b21;--line:#2a2f38}}
*{box-sizing:border-box}
body{margin:0;min-height:100vh;display:grid;place-items:center;padding:1rem;background:var(--bg);color:var(--fg);font:1rem/1.6 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
main{max-width:30rem;width:100%;padding:2rem 1.5rem;background:var(--card);border:1px solid var(--line);border-radius:1rem}
h1{margin:0 0 1rem;font-size:1.75rem;line-height:1.2}
p{margin:0 0 .75rem}
.small{margin:1.25rem 0 0;font-size:.875rem;color:var(--muted)}
</style>
</head>
<body>
<main>
<h1>Payment done</h1>
<p>Thank you. Your premium plan is active in a minute.</p>
<p>Go back to Garmin Companion. The app updates when it gets focus.</p>
<p>If the plan still shows free, open Settings in the app.</p>
<p class="small">You can close this page.</p>
</main>
</body>
</html>
`;

export const checkoutDone = () =>
  new Response(PAGE, {
    headers: {
      "Content-Type": "text/html; charset=utf-8",
      "Cache-Control": "public, max-age=3600",
      "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'",
      "X-Content-Type-Options": "nosniff",
    },
  });
