# Hortus — Project Context for Claude

## Vision
Simulateur d'autosuffisance alimentaire en fruits & légumes, fondé sur l'agroécologie, l'agroforesterie et la permaculture. L'objectif unique mesurable : **maximiser le nombre de jours d'autonomie nutritionnelle complète sur l'année**.

Le moteur simule biologiquement (sol, eau, lumière, plantes, ravageurs, auxiliaires) à partir d'un lieu réel (latitude/climat) et d'une météo procédurale calée sur des normales climatiques.

## Stack
- **Backend** : Rust + Axum + SQLite (à venir) + serde
- **Frontend** : Elm 0.19.1 (vue jardin + calendrier nutritionnel + inspecteur cellule)
- **ML (phase 3)** : tch (libtorch) — RL long-horizon, MCTS avec chance nodes (météo)

## Architecture
```
backend/
  src/
    domain/      # logique pure (sol, plantes, météo, sim, nutrition, garde-manger)
    api/         # handlers HTTP (Axum)
    bin/         # binaires recherche (selfplay, oracle, eval)
    lib.rs
    main.rs
frontend/
  src/Main.elm
  static/{index.html, elm.js, style.css}
```

## Modèle de simulation
- **Tick = 1 jour**
- **Cellule** : NPK, MO, pH, humidité, T° sol, ensoleillement, pression parasitaire
- **Plante** : famille, stade, besoins, rendement, exsudats, nutrition
- **Météo** : procédurale, calée sur normales mensuelles (Météo-France 1991-2020)
- **Photopériode** : exacte, formule astronomique selon latitude
- **Foyer** : N personnes × besoins quotidiens (kcal, macros, micros)
- **Garde-manger** : compartiments (frais / cellier / sec / lacto / conserve / congel)
- **Consommation** : auto-pull stocks → calcule couverture quotidienne → flag déficits

## Métrique de succès
Métrique principale : **jours d'autonomie complète / 365** (apport nutritionnel complet couvert par production + stocks).

Métriques annexes :
- Diversité alimentaire (espèces consommées)
- Efficacité surface (kg/m²/an)
- Effort (heures travail/an)
- Sol légué fin de cycle (MO, biodiversité)

## MVP cible
- Climat tempéré océanique (Paris/Île-de-France)
- Grille hex 0.5m × 100 cellules (5×5m, jardin urbain)
- 30 espèces (légumes-feuilles, racines, fruits, légumineuses, fruitiers)
- Foyer 1 personne adulte
- 3 ans de simulation (1095 ticks)
- Mode bac-à-sable + objectif autonomie

## Leçons héritées de Carcassonne / Qwirkle
- **Méthodologie d'évaluation** : paired evaluation avec bootstrap CI dès le début
- **Oracle bound** : implémenter tôt pour valider que l'objectif est exploitable
- **Greedy ≠ optimal** : ici massivement vrai (monoculture s'effondre, soudure printanière brutale)
- **Action space** : bien modéliser dès le début (~1500-2000 actions/tour)
- **Pas d'AlphaZero d'emblée** : MCTS + rollouts heuristiques d'abord

## Sources de données
- **CIQUAL ANSES** : composition nutritionnelle aliments
- **Météo-France** : normales climatiques 1991-2020 ouvertes
- **Terre Vivante / Au Potager Bio** : fiches culture
- **PVGIS / NASA POWER** : irradiance solaire (optionnel)
