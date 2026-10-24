+++
title = "L'appel"
weight = 3
description = "Saisir, importer et pointer les tireurs ou les équipes."
+++

## Ajouter des tireurs

- **Ajouter** : la fiche du tireur s'ouvre ; la touche Entrée ajoute le tireur depuis n'importe quel champ.
- **Importer** : fichiers FFF, CSV, XML FIE ou classement. **Importer un classement** donne leur classement d'entrée aux tireurs déjà inscrits ; **Modèle CSV** télécharge un fichier avec les colonnes attendues.
- **Coller une copie** : duplique des tireurs (et les membres d'une équipe).

Les colonnes affichées se choisissent dans le menu de la liste. La recherche et les filtres (Tous, Présents, Absents) aident à pointer rapidement.

## Pointer les présents

{{ figure(src="appel.png", caption="L'appel : liste des tireurs, présents et absents.") }}

Cochez la case de présence de chaque tireur, ou utilisez **Présents** / **Absents** pour tout cocher ou tout décocher. Les compteurs indiquent le nombre de présents et d'absents.

## Compétition par équipes

{{ figure(src="equipes-appel.png", caption="L'appel d'une épreuve par équipes : tireurs par équipe et remplaçants.") }}

Chaque tireur appartient à une équipe (colonne *Équipe*). Dans la configuration de l'appel :

| Réglage | Par défaut | Rôle |
|---|---|---|
| Tireurs par équipe | 3 | tireurs d'une équipe dans une rencontre |
| Remplaçants | 1 | remplaçants en plus des tireurs |
| Taille minimale | 3 | en dessous, l'équipe est déclarée absente |

ManageComp vérifie :

- qu'une équipe ne dépasse pas ses tireurs et ses remplaçants : un tireur de plus est **refusé** avec un message ;
- qu'une équipe venue a assez de tireurs présents : sinon un **avertissement** indique « il manque 1 tireur ».

## Fermer l'appel

**Tour suivant** ferme l'appel : les présents passent à la phase suivante de la formule.

## Catégorie générale

La colonne **Catégorie générale** est calculée d'après la date de naissance de chaque tireur (jamais saisie), avec le modèle de l'onglet **Catégories** des préférences : catégories de la FFE fournies (M5 à M20, Seniors, V1 à V4), bascule de la saison au 1er septembre. Le modèle peut être modifié (noms, années de naissance, ajout de catégories, date de bascule), enregistré sous un autre nom et rechargé.

Par rapport à la catégorie de la compétition, un repère de couleur signale, sans rien bloquer :

- **▲** (orange) : tireur surclassé d'une catégorie ;
- **▲▲** (violet) : double surclassement ;
- **⚠** (rouge) : à vérifier (trop âgé pour une compétition de jeunes, trop jeune, ou date de naissance absente).
