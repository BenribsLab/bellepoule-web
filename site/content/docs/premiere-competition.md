+++
title = "Première compétition"
weight = 2
description = "Créer une compétition et la régler."
+++

## Créer la compétition

{{ figure(src="accueil.png", caption="L'écran d'accueil : nouvelle compétition, entraînement des jeunes, compétitions récentes.") }}

Sur l'écran d'accueil, choisissez :

- **Nouvelle compétition** pour une épreuve individuelle ou par équipes ;
- **Nouvel entraînement des jeunes** pour une épreuve en groupes de niveau ([Entraînement des jeunes](@/docs/entrainement-jeunes.md)) ;
- **Ouvrir une compétition** pour reprendre un fichier `.cotcot` (ou importer un fichier FIE, ZIP ou AskFred).

**Nouvelle compétition** demande d'abord la méthode de création : à partir d'un fichier **zip de la FFE** (extranet), d'un fichier **XML**, d'un fichier **AskFred** (`.frd`, une compétition par épreuve), ou une **compétition vide**. La version en ligne propose les mêmes choix.

{{ figure(src="nouvelle-competition.png", caption="Nouvelle compétition : choix de la méthode de création.") }}

La fenêtre des propriétés s'ouvre :

| Champ | À quoi il sert |
|---|---|
| Nom, organisateur, lieu | en-têtes des impressions et des publications |
| Arme, genre, catégorie | règles de la compétition, arbitres proposés |
| Date | impressions, classements |
| Épreuve par équipes | à choisir avant de saisir les tireurs : on ne peut plus le changer ensuite |
| Couleur | repère de la compétition dans l'organisation et sur les écrans |
| Scores : remplissage automatique | **Interdire** par défaut. **Autoriser** ajoute aux poules et aux tableaux un bouton « Remplissage automatique » qui tire des scores au hasard : pour tester une formule sans saisir de résultats |

## Enregistrer

{{ figure(src="proprietes.png", caption="Les propriétés de la compétition et sa formule.") }}

Le bouton **Enregistrer** (ou `Ctrl+S`) sauvegarde le fichier `.cotcot`. Le bouton indique « Enregistré » tant qu'il n'y a pas de modification. À la fermeture, ManageComp ne pose la question que si la dernière version n'a pas été enregistrée.

Les compétitions récentes s'affichent en vignettes sur l'écran d'accueil. Le **✕** d'une vignette la retire de la liste, après confirmation : le fichier n'est pas effacé et s'ouvre toujours avec **Ouvrir une compétition**.

Dans la version en ligne, le **✕** d'une carte retire la compétition de l'accueil sans l'effacer. Le lien **Compétitions retirées de l'accueil**, en bas de la page, les affiche : **Remettre** la replace sur l'accueil, la corbeille la supprime définitivement.

## Plusieurs compétitions

Plusieurs compétitions peuvent être ouvertes en même temps, chacune dans son onglet. L'[organisation](@/docs/organisation.md) (pistes et arbitres) est commune à toutes.
