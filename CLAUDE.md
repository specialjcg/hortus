# Hortus — Project Context for Claude

## Vision

**Hortus** est un **assistant jardinier pour maraîchers amateurs** : calendrier annuel des plantations adapté au climat local, carnet de jardin daté, et coach qui croise météo live et catalogue d'espèces pour suggérer les actions du jour.

Le projet a démarré comme simulateur d'autosuffisance alimentaire (tick journalier, plantes qui poussent, garde-manger, etc.) puis a pivoté vers un outil pratique orienté utilisateur. L'historique ML/MCTS est archivé dans la mémoire auto (`.claude/projects/...memory/qwirkle_lessons.md`) mais n'est plus dans la codebase courante.

## Stack

- **Backend** : Rust 2021 + Axum 0.8 + Tokio
  - Données : SQLite via `rusqlite` (bundled) → `backend/data/hortus.db`
  - Catalogue espèces : JSON → `backend/data/species.json` (~66 espèces)
  - Météo live : Open-Meteo Archive + Forecast via `reqwest`
  - Dates réelles : `chrono::Local::now()`
- **Frontend** : Elm 0.19.1 — SPA avec 3 vues (Coach / Calendrier / Mon jardin). SVG pour la grille, pas de ports.
- **Lancement** : `./start.sh` build backend release + frontend Elm + python http.server sur 8000.

## Architecture

```
backend/
  src/
    domain/         # types métier purs (pas d'I/O)
      time.rs        # SimDate, photopériode
      geo.rs         # Location, ClimateProfile (normales + calcul 5y)
      weather.rs     # DailyWeather, classify_weather
      weather_live.rs# client Open-Meteo async
      species.rs     # Species chargé depuis JSON
    api/
      routes.rs      # Axum router + handlers HTTP
    db.rs            # open(), migrations additives
    journal.rs       # Parcel, Action + CRUD SQLite
    lib.rs           # exports modules
    main.rs          # boot Tokio + Axum
  data/
    species.json    # source du catalogue (versionné)
    hortus.db       # SQLite (gitignored)
frontend/
  src/Main.elm      # mono-fichier, ~2000 lignes
  static/{index.html, elm.js, style.css}
start.sh            # script dev tout-en-un
```

## Endpoints HTTP

- `GET /health`
- `GET /cities` — liste des villes
- `GET /species` — catalogue complet
- `GET /calendar?city=X&refresh_climate=true` — espèces avec fenêtres locales décalées
- `GET /forecast?city=X` — météo J+7 Open-Meteo
- `GET /action-kinds` — types d'action acceptés
- `GET/POST /parcels`, `PUT/DELETE /parcels/{id}`
- `GET/POST /actions`, `PUT/DELETE /actions/{id}`

## Modèle calendaire

- **Espèce** : id, nom FR/latin, famille, cycle, catégorie, difficulté, fenêtres (indoor/direct/transplant/harvest), profondeur, espacement, notes, compagnons/antagonistes.
- **Calibration locale** : les fenêtres nominales (référence Paris) sont décalées par `spring_shift_days()` qui compare les Tmin mars-avril entre climat cible et référence.
- **Climat** : soit statique (normales 1991-2020 hard-codées par ville), soit calculé (Open-Meteo Archive 5 ans + moyennes mensuelles).

## Journal (carnet)

- **Parcel** : nom, surface, exposition, notes sol + position grille (`grid_x/y/w/h`, couleur). Legacy, l'UI actuelle ne l'expose plus comme entité centrale.
- **Action** : date, type (`semis_direct`, `semis_abri`, `repiquage`, `arrosage`, `paillage`, `compost`, `recolte`, `traitement`, `arrachage`, `note`), `parcel_id` optionnel, `species_id` optionnel, `quantity_g`, `notes`, **`grid_x/y`** = coordonnées pixel sur le terrain ou l'abri.
- Les "plants" visuels sont dérivés des actions `semis_direct`/`repiquage` (terrain) et `semis_abri` (abri).
- État dérivé d'un plant = `f(date_semis, days_to_harvest_espèce, today)` → Sown / Growing / Mature / Harvested.

## Frontend — 3 vues

1. **🎯 Coach** : à semer ce jour (fenêtres actives, tri par jours restants), alertes météo J+7 (gel/canicule/orage croisées avec espèces sensibles plantées), ta semaine (stats), conseil saisonnier.
2. **📅 Calendrier** : ruban annuel par espèce avec 4 bandes (abri/direct/repiquage/récolte), panneau saisonnier, détail espèce au clic.
3. **📓 Mon jardin** : terrain continu 800×560 + abri 800×150, palette d'espèces (⭐ recos du jour), drag & drop pour poser/déplacer, clic simple → menu contextuel (actions rapides), panneau actions en lot.

## Conventions Rust

Référence complète : `.claude/skills/rust-skills/CLAUDE.md` (179 règles).

**À respecter strictement** :
- `err-no-unwrap-prod` : pas de `.unwrap()` en prod. Utiliser `.expect("reason")` pour les bugs impossibles (ex. Mutex empoisonné), ou `?` + `thiserror/anyhow` pour tout le reste.
- `proj-flat-small` : modules plats tant que le projet reste petit (< 5k lignes). Pas de sous-modules inutiles.
- `test-cfg-test-module` : tests unitaires dans `#[cfg(test)] mod tests { use super::*; }` au bas de chaque module.
- `name-funcs-snake`, `name-types-camel`, `name-consts-screaming` : conventions standard.
- `own-borrow-over-clone` : emprunter par `&T` quand possible ; `clone()` doit être explicite.
- `api-parse-dont-validate` : valider en entrée (HTTP handlers), ensuite les types portent l'invariant.

**Configuration release** (voir `Cargo.toml`) :
```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
```

## Commandes utiles

```bash
# Dev tout-en-un
./start.sh                      # build + run backend + frontend sur 8000

# Backend uniquement
cd backend
cargo build --release --bin hortus-backend
cargo test                      # tests unit + intégration

# Frontend uniquement
cd frontend
./node_modules/.bin/elm make src/Main.elm --output=static/elm.js
python3 -m http.server 8000 --directory static
```

## Sources de données

- **Catalogue espèces** : `backend/data/species.json` (source de vérité — éditer là pour ajouter ou corriger une espèce).
- **Normales climatiques** : Météo-France 1991-2020 (hard-codées dans `geo.rs` pour 9 villes).
- **Météo live** : Open-Meteo (gratuit, sans clé). Archive API pour le passé + Forecast API pour J+7.
- **Fiches culture** : Terre Vivante / Au Potager Bio / Rustica (sources citées lors de la saisie JSON).
