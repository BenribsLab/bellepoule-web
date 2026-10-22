+++
title = "Hosting ManageComp"
slug = "hosting"
weight = 14
description = "Install the server with Docker behind Apache."
+++

The whole server (API, online version, website and documentation) is deployed with a single Docker command.

## Requirements

- Docker and Docker Compose;
- a front web server for HTTPS (Apache, for example with ISPConfig and Let's Encrypt).

## Deployment

```sh
git clone <ManageComp Web repository>
cd managecomp-web
cp .env.example .env      # then set BP_ADMIN_KEY (at least 32 characters)
docker compose up -d --build
```

Containers only listen on the machine itself (`127.0.0.1`): only Apache is exposed to the Internet.

| Container | Local port | Domain |
|---|---|---|
| client (online version, API under `/api`) | 8090 | `app.`, `api.`, `arbitre.`, `live.` |
| site (presentation and documentation) | 8091 | main domain |

## Apache directives (ISPConfig)

For each site, with Let's Encrypt enabled, in the Apache directives:

```apache
ProxyPreserveHost On
ProxyPass /.well-known/acme-challenge !
ProxyPass / http://127.0.0.1:8090/
ProxyPassReverse / http://127.0.0.1:8090/
RequestHeader set X-Forwarded-Proto "https"
```

(use 8091 instead of 8090 for the main domain). Required modules: `proxy`, `proxy_http`, `headers`.

## Club keys

The administration key (`BP_ADMIN_KEY`) creates each club's key:

```sh
curl -X POST https://api.managecomp.org/api/admin/clients \
  -H "x-api-key: <administration key>" \
  -H "content-type: application/json" \
  -d '{"name":"My club"}'
```

The club key is shown only once: write it down.

## Updates

```sh
git pull
docker compose up -d --build
```

Data (competitions, keys) lives in the `bellepoule-data` Docker volume and is kept.
