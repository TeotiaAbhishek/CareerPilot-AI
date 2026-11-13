> **CareerPilot v2 is being built** in `backend/` (Rust) and `web/` (Next.js), next to the current Python app, which keeps running until v2 reaches parity.
>
> **Run v2 locally:**
> 1. `docker compose up -d` (Postgres + pgvector on port 5433)
> 2. `cd backend && cp .env.example .env && cargo run` (API on http://localhost:8080)
> 3. `cd web && cp .env.example .env.local && npm install && npm run dev` (UI on http://localhost:3000)
