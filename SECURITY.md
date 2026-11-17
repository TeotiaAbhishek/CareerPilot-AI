# Security

CareerPilot is a single-person app that runs on your own computer. This page explains what it protects, how, and what it leaves to you.

## Reporting a problem

Please open a [GitHub security advisory](https://github.com/TeotiaAbhishek/CareerPilot-AI/security/advisories/new) instead of a public issue. I'll reply within a week.

## What it protects

| What | How |
|---|---|
| Passwords for job sites | Encrypted with AES-256-GCM before they reach the database. The key (`VAULT_KEY`) lives only in `backend/.env`, never in the database or the repo |
| Your contact details | Never sent to the model. Prompts get your resume's evidence and your work rights only. Name, phone, email and links are added by code afterwards |
| Gmail | Read-only access (`gmail.readonly`), only to read sign-up codes. Emails are drafted in the app; you send them yourself |
| Job ads | Stored as sanitised HTML (`ammonia`), so a script in an ad can't run in the app |
| The model's answers | Shown as Markdown without raw HTML. Links only open when they point at an app page or `https://` |
| Claude and OpenAI (optional) | Off until you add a key. They get the job ad and your resume with contact lines removed by code, never your details, passwords or form pages: the apply agent only runs on your local model. A monthly cap per provider stops runaway costs |
| The Ask assistant | Its tools only read, and each one runs inside a `READ ONLY` database transaction |

## What the agent will never do

- **Click Submit.** It fills in the form and stops; you review and submit.
- **Solve a CAPTCHA.** The run stops and asks you.
- **Send an email.** You send it from Gmail.

## How it's exposed

- The API listens on `127.0.0.1:8080` only, and the browser worker on `127.0.0.1:8090`. Neither is reachable from your network.
- The web app passes `/api/...` on to the API, so your browser (and your phone) only ever talks to the web app.
- **There is no login page.** Anyone who can open the web app can see your data and reveal saved passwords. So never put it on the public internet. To reach it from your phone, use a private network such as [Tailscale](https://tailscale.com) (see the README), which only your own signed-in devices can use.

## Your part

- Keep `backend/.env` private and backed up. Without `VAULT_KEY`, saved passwords can't be decrypted. Never paste it into a chat, an issue or a screenshot.
- Keep your resume in the database (Profile page) or in the git-ignored `data/` folder, never anywhere else in the repo.
- Gmail OAuth client ID and secret, and the Claude and OpenAI API keys, go only in `backend/.env`. If a key leaks, delete it in the provider's console and make a new one.

## Checks

- CI runs `cargo clippy -D warnings`, ESLint and all tests on every pull request.
- `cargo audit` (backend) and `npm audit --omit=dev` (web, browser worker) are clean. Advisories that can't affect CareerPilot are listed, with the reason, in `backend/.cargo/audit.toml`.
