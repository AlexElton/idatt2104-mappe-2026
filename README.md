# Nettverk – RGA Collaborative Editor

[![CI](https://github.com/AlexElton/idatt2104-mappe-2026/actions/workflows/ci.yml/badge.svg)](https://github.com/AlexElton/idatt2104-mappe-2026/actions/workflows/ci.yml)

Siste CI-kjøring: <https://github.com/AlexElton/idatt2104-mappe-2026/actions/workflows/ci.yml>

## Introduksjon

Nettverk er en proof-of-concept kollaborativ teksteditor bygget på **Replicated Growable Array (RGA)** CRDT i Rust. Løsningen viser flere klienter som kan redigere samme dokument samtidig og slå det sammen til en tekst uten Operational Transformation eller låsing.

Arkitekturen er klient-server:

CRDT-kjernen ligger i `crates/core` og kompileres både som vanlig Rust-bibliotek og som WebAssembly for frontend. Backend holder en in-memory dokumentøkt og broadcaster validerte operasjoner til tilkoblede klienter. Frontend bruker samme Rust-implementasjon gjennom WASM, slik at klientene selv kan anvende lokale og eksterne operasjoner.

## Implementert funksjonalitet

TODO: Skriv inn noe her

## Fremtidig arbeid og kjente mangler

- Dokumenttilstand lagres kun i minnet. Restart av backend sletter dokumentet.
- Det finnes bare en global dokumentøkt.
- Ingen autentisering, autorisasjon eller brukerprofiler.
- Tombstone-garbage-collection er demonstrativ. I en produksjonsløsning må man vite at alle aktive/aktuelle replikaer har sett slettingene før tombstones fjernes.
- Presence er begrenset til markørposisjon, ikke full selection/ranges eller brukeridentitet.
- Kun WebSocket-transport er implementert. WebRTC eller annen transport kan legges til via `CollaborationTransport`.

## Eksterne avhengigheter

### Rust/backend

- `tokio` – async runtime for backend.
- `axum` – HTTP-server, routing og WebSocket upgrade.
- `futures-util` – stream/sink-hjelpere for WebSocket-håndtering.
- `serde` – serialisering/deserialisering av operasjoner og meldinger.
- `serde_json` – JSON wire-format over WebSocket.
- `wasm-bindgen` – eksponerer Rust-kjernen til JavaScript/WASM.
- `serde-wasm-bindgen` – konverterer Rust/Serde-verdier til JavaScript-verdier.
- `console_error_panic_hook` – bedre panic-feilmeldinger i browser console for WASM.

### Frontend

- `react` og `react-dom` – UI-rammeverk.
- `@codemirror/state` og `@codemirror/view` – teksteditor, editor-state og cursor widgets.
- `zustand` – enkel klient-side state store.
- `zod` – runtime-validering av collaboration-protokollmeldinger.
- `rga-core` – workspace-pakken som peker på prosjektets egen Rust/WASM CRDT-kjerne.

### Verktøy

- `pnpm` – package manager.
- `nx` – kjører frontend/backend/wasm targets i monorepoet.
- `vite` – frontend dev server og bundling.
- `@vitejs/plugin-react` – React-støtte i Vite.
- `vite-plugin-wasm` og `vite-plugin-wasm-hmr` – lasting og hot reload av WASM-kjernen.
- `tailwindcss` og `@tailwindcss/vite` – styling.
- `wasm-pack` – bygger Rust-kjernen til WebAssembly.
- `bacon` – restartende Rust dev-runner for backend.
- `oxlint` og `oxfmt` – JavaScript/TypeScript linting og formattering.
- `cargo clippy` og `cargo fmt` – Rust linting og formattering.

## Installasjon

Forutsetninger:

- Rust stable med Cargo: <https://rustup.rs/>
- Node.js 22 eller nyere
- pnpm 10.x
- `wasm-pack`
- `bacon` hvis du vil bruke `pnpm dev:backend`/`pnpm dev`

```bash
git clone https://github.com/AlexElton/idatt2104-mappe-2026.git
cd idatt2104-mappe-2026
cargo install wasm-pack --locked
cargo install bacon --locked
pnpm install
```

## Bruk

Start backend, bygg WASM i dev-modus og start frontend:

```bash
pnpm dev
```

Backend lytter på <http://localhost:3000>. Vite viser frontend-URL-en i terminalen, normalt <http://localhost:5173>. Frontend proxier `/api` og `/ws` til backend.

Du kan også starte delene hver for seg:

```bash
pnpm dev:backend
pnpm dev:web
```

### Demo av CRDT-konvergens

1. Start appen med `pnpm dev`.
2. Åpne frontend i forskjellige nettlesere eller bruk incognito-modues (userid lagres i localstorage).
3. Skriv tekst i begge vinduer og observer at dokumentene synkroniseres.
4. Skru av `Sync` i en fane.
5. Skriv videre i begge faner. Lokale og eksterne operasjoner blir buffered.
6. Skru `Sync` på igjen. Fanene skal konvergere til samme tekst.
7. Slett noen tegn og se tombstones i `RGA Tree`-panelet.
8. Trykk `Clear deleted` for å demonstrere tombstone-opprydding. (Dette kan forårsake noen feil da en korrekt implementasjon av GC ikke er implementert.)

## Bygg

```bash
pnpm build:web
pnpm build:backend
```

Eller bygg alt:

```bash
pnpm build
```

`pnpm build:web` bygger først `rga-core` til WASM og bygger deretter React-appen til `apps/web/dist/`. `pnpm build:backend` bygger Rust-backend i release-modus.

## Tester og kvalitetssjekker

Rust-tester:

```bash
cargo test --workspace
```

TypeScript typecheck:

```bash
pnpm --filter web exec tsc --noEmit
```

Lint og format check:

```bash
pnpm lint
pnpm format:check
```

CI kjører installasjon, lint, format check og build på push/pull request mot `main`.

## API og protokoll

Wire-protokollen er JSON over WebSocket. Klienten sender:

- `hello` – registrerer `replica_id` og `session_id`
- `ops` – sender en liste CRDT-operasjoner
- `presence` – sender markørposisjon
- `garbage_collect` – ber andre klienter rydde tombstones

Serveren sender:

- `hydrate` – initial operasjonslogg, presence og klientantall
- `ops` – CRDT-operasjoner fra andre klienter
- `presence` – samlet presence-state
- `garbage_collect` – beskjed om tombstone-opprydding
