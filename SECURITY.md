# Security policy

Finoinc holds people's bank statements, so a security bug here is a privacy bug.
Reports are taken seriously and handled privately.

## Reporting a vulnerability

**Do not open a public issue.** Use one of these instead:

- **Preferred:** [report it privately on GitHub](https://github.com/MateusFonseK/finoinc/security/advisories/new)
  (Security tab → *Report a vulnerability*).
- Or email **mateusfonsecafs@gmail.com** with `[finoinc security]` in the subject.

Include the version (Finoinc menu → About Finoinc), your macOS version, and the steps to
reproduce. You will get an answer within 7 days. Once a fix ships, the advisory
is published and you are credited unless you ask not to be.

## Supported versions

Only the [latest release](https://github.com/MateusFonseK/finoinc/releases/latest)
gets fixes. There is no auto-update, so upgrading means downloading the new `.dmg`.

## What is in scope

The claims the app makes are the security boundary:

- **No network by default.** Any request the app makes that the
  [privacy table](README.md#-privacy--the-actual-claim) does not list.
- **The MCP server.** It must be reachable only from `127.0.0.1`, must refuse
  any request carrying a web `Origin`, and must never run a tool that is
  switched off or read past the history window the owner chose.
- **Tax-id lookup.** When on, it must send only CNPJ digits — never amounts,
  descriptions or anything else.
- **Statement import.** A crafted `.ofx` file that crashes the app, corrupts the
  database or runs code.
- **The webview.** Anything that bypasses the content security policy or reaches
  a Tauri command the UI does not expose.

## What is out of scope

- Gatekeeper's warning on first launch: the app is not notarized, and that is
  documented in the [install steps](README.md#-install).
- Another process running as your user on the same Mac. It can already read the
  database file directly, so the MCP server having no token is a deliberate
  trade, not a vulnerability.
- What an AI agent does with data it read through the MCP server. The server
  sends nothing anywhere; the agent you connect does.
