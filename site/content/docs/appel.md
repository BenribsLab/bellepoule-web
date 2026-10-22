+++
title = "L'appel"
weight = 3
description = "Saisir, importer et pointer les tireurs ou les équipes."
+++

## Ajouter des tireurs

- **Ajouter** : la fiche du tireur s'ouvre ; la touche Entrée ajoute le tireur depuis n'importe quel champ.
- **Importer** : fichiers FFF, CSV, XML FIE ou classement.
- **Coller une copie** : duplique des tireurs (et les membres d'une équipe).

Les colonnes affichées se choisissent dans le menu de la liste. La recherche et les filtres (Tous, Présents, Absents) aident à pointer rapidement.

## Pointer les présents

Cochez la case de présence de chaque tireur, ou utilisez **Présents** / **Absents** pour tout cocher ou tout décocher. Les compteurs indiquent le nombre de présents et d'absents.

## Compétition par équipes

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
