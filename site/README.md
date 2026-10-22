# Site public et documentation de ManageComp

Site statique généré par [Zola](https://www.getzola.org) (0.21), servi par
nginx sans droits root. Il est construit et lancé avec le reste de
l'infrastructure :

```sh
docker compose up -d --build      # depuis la racine du projet
```

Il répond en local sur `127.0.0.1:8091` ; Apache le sert en HTTPS sur
`managecomp.org` (voir `content/docs/serveur.md`).

## Organisation

| Dossier | Contenu |
|---|---|
| `config.toml` | réglages, adresses de l'infrastructure, **libellés traduits** de l'interface |
| `content/` | pages (Markdown) : accueil, téléchargement, `docs/` |
| `templates/` | gabarits (Tera) |
| `static/` | feuille de style, logo |
| `nginx.conf` | service et en-têtes de sécurité (aucun script autorisé) |

## Langues

Le français est la langue par défaut ; l'anglais est servi sous `/en/`.
Chaque page existe une fois par langue :

```
content/docs/poules.md      ← français
content/docs/poules.en.md   ← anglais (slug = "pools" dans l'en-tête)
```

### Ajouter une langue (exemple : espagnol)

1. Dans `config.toml`, ajouter un bloc `[languages.es]` et sa table
   `[languages.es.translations]` avec les mêmes clés que `[translations]`.
2. Dans `templates/*.html`, ajouter `"es"` aux listes `["fr", "en"]` du
   sélecteur de langue.
3. Traduire chaque page : `content/**/nom.es.md` (avec un `slug` espagnol si
   on le souhaite).
4. `docker compose up -d --build`.

Une page non traduite n'apparaît pas dans la langue concernée ; le sélecteur
de langue ne propose que les traductions qui existent.

## Aperçu en local

```sh
docker run --rm -v "$PWD:/site" -w /site -p 1111:1111 \
  ghcr.io/getzola/zola:v0.21.0 serve --interface 0.0.0.0 --base-url localhost
```

puis <http://localhost:1111>.
