# apps/login

`garmin-companion-login`, the sign-in helper of the desktop app. Read
`apps/desktop/CLAUDE.md` (the `apps/login` item under Layout, and the platform
gotchas) and the root `CLAUDE.md` first.

- It prints the ticket on stdout. Exit 0 is a ticket, 1 is no ticket, 2 is bad
  arguments. Do not print anything else on stdout.
- It accepts only https URLs on `garmin.com` or a subdomain.
- On macOS, the Edit menu has no Quit, because Quit would exit 0 with no
  ticket.
