# ManageComp — serveur

ManageComp est un logiciel libre de gestion de compétitions d'escrime, dérivé de
**BellePoule** (© 2009-2013 Yannick Le Roux), publié sous **GNU GPL version 3 ou
ultérieure** (voir `LICENSE` et `CREDITS.md`).

Ce dépôt contient tout le serveur, déployé avec Docker :

| Dossier | Rôle | Technologie | Domaine |
|---|---|---|---|
| `bellepoule-web-api/` | API : compétitions (appel, poules, tableaux, classements, entraînement des jeunes, équipes), organisation, arbitres sur téléphone, scores en direct. Contient sa propre copie du cœur (`bp-core`). | Rust, axum | `api.managecomp.org` |
| `bellepoule-client/` | Version en ligne dans le navigateur ; relaie `/api` vers l'API. | Rust → WebAssembly (Leptos), nginx | `app.`, `arbitre.`, `live.` |
| `site/` | Site public et documentation, multilingue. | Zola, nginx | `managecomp.org` |

## Lancer

```sh
cp .env.example .env          # puis renseigner BP_ADMIN_KEY (openssl rand -hex 32)
docker compose up -d --build
```

- Version en ligne : <http://127.0.0.1:8090> ; site : <http://127.0.0.1:8091>.
- Les ports n'écoutent que sur la machine : Apache (ISPConfig, Let's Encrypt) sert les
  domaines en HTTPS. Directives : `site/content/docs/serveur.md`.
- Les compétitions sont enregistrées au format `.cotcot` (compatible BellePoule) dans le
  volume Docker `bellepoule-data` : elles survivent aux redémarrages et aux mises à jour.

## Sécurité

- conteneurs sans droits root, en lecture seule (sauf `/data` et `/tmp`), sans capacités
  Linux, sans élévation de privilèges ;
- en-têtes de sécurité (CSP stricte, HSTS, nosniff, pas d'affichage en cadre) ; dans le
  client, seul le script de démarrage, identifié par son empreinte, peut s'exécuter ;
- pas de CORS : le navigateur n'appelle l'API que par le même domaine ;
- clés des clubs enregistrées sous forme d'empreinte SHA-256, comparées en temps constant ;
  clé d'administration de 32 caractères au moins ;
- vérification des dépendances : `cargo audit` (base RustSec).

## Historique : chrono et cartons des arbitres (22/10/2026)

Les sources de l'API et du client incluent désormais les paramètres de formule du chronomètre :
poules 5 touches / 1 × 3 minutes, nouveaux tableaux 15 touches / 3 × 3 minutes. Durée et manches
restent modifiables dans les réglages de phase, puis sont transmises à Android dans `timing`.
Les valeurs explicitement présentes dans les fichiers existants sont conservées.

`POST /api/smartpoule/phone/score` accepte maintenant un champ `officiating`, avec ou sans résultat
`values`, pour enregistrer chrono et cartons pendant le match. Les anciens clients à scores seuls
restent acceptés. Le relais conserve les saisies dans le dossier privé du club et vérifie les
affectations par arbitre/match. Les feuilles imprimables affichent les cartons du match concerné.
Le premier nom de la feuille est vert par défaut ; l'inversion sur Android ne change pas l'ordre API.

La copie `bp-core` a été synchronisée depuis le bureau. Détails et état :
`../Bellepoule Rust/CHRONO_CARTONS.md` et `../Bellepoule Rust/smartpoule-android/README.md`.
**Cette évolution n'a pas été compilée/testée ni déployée par l'agent**, sur demande de l'utilisateur.
L'API, le bureau et l'APK doivent être reconstruits ensemble par l'utilisateur ; aucun conteneur
n'a été redémarré pendant ces modifications.

## Réglages

| Variable | Conteneur | Défaut | Rôle |
|---|---|---|---|
| `BP_LANGUAGE` | api | `fr` | Langue des libellés |
| `BP_DATA_DIR` | api | `/data` | Dossier des compétitions |
| `BP_API_URL` | client | `http://api:8080` | Adresse de l'API vue par le client |

Le port public est réglé par la variable `BP_PORT` (8090 par défaut).

## Sécurité : clés d'API

Chaque club (ou comité) a sa **clé d'API** : il ne voit et ne modifie que ses propres compétitions,
rangées dans son dossier (`/data/clients/<client>/`). Les clés sont enregistrées hachées (SHA-256).

1. Choisir une clé d'administration (16 caractères au moins) au lancement :
   `BP_ADMIN_KEY=une-longue-cle-secrete docker compose up -d`
2. Créer la clé d'un club (elle n'est affichée qu'une fois) :
   ```sh
   curl -X POST http://localhost:8090/api/admin/clients         -H "x-api-key: une-longue-cle-secrete" -H "content-type: application/json"         -d '{"name":"CE Yerres"}'
   ```
3. Le club saisit sa clé dans l'écran de connexion du site.

Administration : `GET /api/admin/clients` (liste), `POST /api/admin/clients/{id}/key` (nouvelle clé),
`DELETE /api/admin/clients/{id}` (retire l'accès ; les compétitions restent sur le disque).

## API

Toutes les adresses (sauf `/api/health`) demandent l'en-tête `X-Api-Key` (ou `Authorization: Bearer`).

| Méthode | Adresse | Rôle |
|---|---|---|
| GET | `/api/me` | Club de la clé |
| GET | `/api/meta` | Listes : armes, sexes, catégories, niveaux, couleurs |
| GET / POST | `/api/contests` | Compétitions du club / nouvelle compétition |
| POST | `/api/contests/import` | Import d'un fichier `.cotcot` |
| GET / DELETE | `/api/contests/{id}` | Résumé / suppression |
| GET / PUT | `/api/contests/{id}/properties` | Propriétés |
| GET | `/api/contests/{id}/cotcot` | Fichier BellePoule |
| GET | `/api/contests/{id}/schedule` | Formule, phase en cours, erreur bloquante |
| POST | `/api/contests/{id}/next` · `/previous` | Tour suivant / précédent |
| GET | `/api/contests/{id}/checkin` | Appel : tireurs, présents, absents |
| GET | `/api/contests/{id}/checkin/form` | Champs de la fiche d'un tireur (avec leurs listes) |
| POST | `/api/contests/{id}/checkin/fencers` | Nouveau tireur |
| GET / PUT / DELETE | `/api/contests/{id}/checkin/fencers/{ref}` | Fiche d'un tireur |
| PUT | `/api/contests/{id}/checkin/attending` | Présence (tous ou certains) |
| POST | `/api/contests/{id}/checkin/import?name=fichier.fff` | Import FFF / CSV |
| POST / DELETE | `/api/contests/{id}/schedule/stages[/{index}]` | Ajout / retrait d'une phase de la formule |
| GET | `/api/contests/{id}/stages/{index}` | Vue d'une phase (groupes, composition, feuilles de poule, tableau, barrage, scission, classement) |
| POST | `/api/contests/{id}/stages/{index}` | Action sur une phase (`{"action": …}` : score, statut, déplacement, réglages…) |
| GET | `/api/contests/{id}/export/{html,fff,frd}` | Exports |
| GET | `/api/contests/{id}/stages/{index}/match_sheets` | Feuilles de match imprimables (barrage, tableaux) |
| GET / POST | `/api/organisation` | Organisation : salle, pistes, frise, lots de matchs, arbitres / actions (`{"action": …}`) |
| POST | `/api/organisation/import?weapon=E&name=arbitres.fff` | Import d'arbitres (FFF / CSV) |

## Code partagé avec le logiciel de bureau

`bp-core` (cœur), `marshaller` (logique d'organisation, sans interface) et `bp-net` (type
`Message` seul) sont des **copies** du dépôt « Bellepoule Rust », pour que ce projet reste autonome.
Après une évolution du logiciel de bureau : `python tools/sync_web.py` (dans « Bellepoule Rust »),
puis reconstruire (`docker compose up -d --build`). Ne pas modifier ces copies à la main.

L'organisation (pistes, arbitres) fonctionne comme la vue intégrée du bureau : une salle par club,
alimentée par les compétitions en cours ; les pistes, heures et arbitres affectés reviennent dans
les poules et les tableaux (feuilles comprises). Données : `<data>/clients/<club>/organisation/`.

## Développement sans Docker

```sh
# API (port 8080, données dans ./data)
cd bellepoule-web-api
BP_DATA_DIR=./data BP_RESOURCES=./resources cargo run

# Client (outil trunk : https://trunkrs.dev), relayé vers l'API locale
cd bellepoule-client
trunk serve --proxy-backend=http://localhost:8080/api/
```
