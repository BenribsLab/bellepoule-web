+++
title = "Héberger ManageComp"
weight = 14
description = "Installer le serveur avec Docker derrière Apache."
+++

Tout le serveur (API, version en ligne, site et documentation) se déploie avec une seule commande Docker.

## Prérequis

- Docker et Docker Compose ;
- un serveur web en frontal pour le HTTPS (Apache, par exemple avec ISPConfig et Let's Encrypt).

## Déploiement

```sh
git clone <dépôt ManageComp Web>
cd managecomp-web
cp .env.example .env      # puis renseigner BP_ADMIN_KEY (32 caractères au moins)
docker compose up -d --build
```

Les conteneurs n'écoutent que sur la machine elle-même (`127.0.0.1`) : seul Apache est exposé sur Internet.

| Conteneur | Port local | Domaine |
|---|---|---|
| client (version en ligne, API sous `/api`) | 8090 | `app.`, `api.`, `arbitre.`, `live.` |
| site (présentation et documentation) | 8091 | domaine principal |

## Directives Apache (ISPConfig)

Pour chaque site, avec Let's Encrypt activé, dans les directives Apache :

```apache
ProxyPreserveHost On
ProxyPass /.well-known/acme-challenge !
ProxyPass / http://127.0.0.1:8090/
ProxyPassReverse / http://127.0.0.1:8090/
RequestHeader set X-Forwarded-Proto "https"
```

(remplacer 8090 par 8091 pour le domaine principal). Modules nécessaires : `proxy`, `proxy_http`, `headers`.

## Clés des clubs

La clé d'administration (`BP_ADMIN_KEY`) permet de créer la clé de chaque club :

```sh
curl -X POST https://api.managecomp.org/api/admin/clients \
  -H "x-api-key: <clé d'administration>" \
  -H "content-type: application/json" \
  -d '{"name":"Mon club"}'
```

La clé du club n'est affichée qu'une fois : notez-la.

## Mises à jour

```sh
git pull
docker compose up -d --build
```

Les données (compétitions, clés) sont dans le volume Docker `bellepoule-data` et sont conservées.
