# Nettverk – RGA Collaborative Editor

## Introduksjon

Dette er et proof-of-concept kollaborativ teksteditor bygget på **Replicated Growable Array (RGA)** CRDT i Rust. Løsningen viser flere klienter som kan redigere samme dokument samtidig og ende med samme tekst uten Operational Transformation eller låsing.

Arkitekturen er klient-server:

CRDT-kjernen ligger i `crates/core` og kompileres både som vanlig Rust-bibliotek og som WebAssembly for frontend. Backend holder en in-memory dokumentøkt og broadcaster operasjoner til klienter som er koblet til backend serveren. Frontend bruker samme Rust-implementasjon gjennom WASM, slik at klientene selv kan gjøre lokale og eksterne operasjoner uten at de to implementasjonene gjør forskjellig logikk. Selve editoren bruker RGA-delen av kjernen.

## Implementert funksjonalitet

- Rust-basert CRDT-kjerne i `crates/core` med:
  - RGA for kollaborativ tekst redigering:
    - Lamport-baserte operasjons-ID-er
    - Tombstones og deterministisk håndtering av samtidige inserts.
  - En liten LWW-registertype for metadata-eksempler (ikke i bruk i applikasjonen)
- WebAssembly-eksport av `rga-core`, pakket som workspace-pakken `rga-core` og brukt direkte fra React-appen.
- Axum-backend i `crates/backend` med en in-memory dokument, WebSocket-endepunkt, hydrering av nye klienter, enkel "cursor position" og broadcast av validerte CRDT-operasjoner.
- React/CodeMirror-frontend i `apps/web` med kollaborativ editor, sync av/på for å demonstrere offline buffering, RGA tree-debugpanel og en manuell naiv tombstone-opprydding.

## Hvordan frontend bruker `rga-core`

Hver nettleserfane oppretter en `Replica` gjennom WASM-wrapperen. `replica_id` lagres i `localStorage`, mens `session_id` lages på nytt for hver runtime. CodeMirror-endringer oversettes til tegnvise `localInsert`/`localDelete`-operasjoner. Operasjonene sendes over WebSocket når sync er på, eller legges i lokal kø når sync er av.

Når klienten mottar `hydrate`, bygges en ny lokal replika fra serverens operasjonslogg. Vanlige `ops`-meldinger kjøres gjennom `applyRemoteBatch`, og editor-teksten patches fra `replica.text()`. Debugpanelet bruker `rgaTree()` for å vise ankere, lenket rekkefølge og tombstones.

> **PS:** her er det også viktig å presisere at hver nettleserfane lagrer sin replica_id'en i `localStorage`. Dersom du da skal teste redigering med to klienter er det viktig at du enten bruker to **forskjellige nettlesere** eller at du bruker **incognito-modus**

## Fremtidig arbeid og kjente mangler

- Dokumenttilstand lagres kun i minnet. Restart av backend sletter dokumentet.
- Det finnes bare en global dokumentøkt.
- Tombstone-garbage-collection er demonstrativ. I en produksjonsløsning må man vite at alle aktive/aktuelle replikaer har sett slettingene før tombstones fjernes.
- Store dokumenter og store paste-operasjoner kan gi mye JSON/WASM-overhead fordi modellen sender én operasjon per tegn. Chunking, binært format eller flere mindre RGA-replikaer per avsnitt kan være bedre.
- Presence er begrenset til markørposisjon, ikke full selection/ranges eller brukeridentitet.
- Kun WebSocket-transport er implementert. WebRTC eller annen transport kan legges til via `CollaborationTransport`.

## Eksterne avhengigheter

### Rust/backend

- `tokio` – async runtime for backend.
- `axum` – HTTP-server, routing og WebSocket upgrade.
- `serde` – serialisering/deserialisering av operasjoner og meldinger.
- `serde_json` – JSON wire-format over WebSocket.
- `wasm-bindgen` – eksponerer Rust-kjernen til JavaScript/WASM.

### Frontend

- `react` – UI-rammeverk.
- `codemirror` – teksteditor, editor-state og cursor widgets.
- `zustand` – enkel klient-side state store.
- `zod` – runtime-validering av collaboration-protokollmeldinger.
- `rga-core` – workspace-pakken som inneholder Rust/WASM CRDT-kjernen.

### Verktøy

- `pnpm` – package manager [finner mer info her](https://pnpm.io/).
  - Det kan hende det er mulig å bruke `npm` her, men det er absolutt ingen garanti.
- `nx` – kjører frontend/backend/wasm targets i monorepoet.
- `vite` – frontend dev server og bundling.
- `vite-plugin-wasm` og `vite-plugin-wasm-hmr` – lasting og hot reload av WASM-kjernen.
- `tailwindcss` – styling.
- `wasm-pack` – bygger Rust-kjernen til WebAssembly.
- `bacon` – restartende Rust dev-runner for backend (ikke et krav for å kunne kjøre) [finner mer info her](https://github.com/canop/bacon).
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

### Kjøre uten Nx og Bacon

Hvis du ikke vil bruke Nx eller Bacon, kan du starte Vite og backend direkte:

Terminal 1 – backend/Cargo:

```bash
cd crates/backend
cargo run
```

Etter at backend startet:

Terminal 2 – frontend/Vite:

```bash
cd apps/web
pnpm dev
```

Åpne Vite-URL-en som vises i terminalen, normalt <http://localhost:5173>. Frontend proxier fortsatt `/api` og `/ws` til backend på <http://localhost:3000>.

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

## Dokumentasjon

Rustdoc-kommentarene i `crates/core` og `crates/backend` er skrevet for å fungere som en liten teknisk wiki for prosjektet. Generer dokumentasjonen lokalt med:

```bash
cargo doc --workspace --no-deps --open
```

## API og protokoll

Wire-protokollen er JSON over WebSocket. Klienten sender:

- `hello` – registrerer `replica_id` og `session_id`
- `ops` – sender en liste CRDT-operasjoner
- `presence` – sender markørposisjon
- `garbage_collect` – ber serveren kjøre demonstrativ tombstone-opprydding og varsle andre klienter

Serveren sender:

- `hydrate` – initial operasjonslogg, presence og klientantall
- `ops` – CRDT-operasjoner fra andre klienter
- `presence` – samlet presence-state
- `garbage_collect` – beskjed om å kjøre demonstrativ tombstone-opprydding lokalt
