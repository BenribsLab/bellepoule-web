+++
title = "Télécharger"
template = "simple.html"
description = "Télécharger ManageComp pour Windows ou Linux (PC, Raspberry Pi), ou utiliser la version en ligne."
+++

## ManageComp pour Windows

1. Téléchargez la dernière version (archive `.zip` pour Windows 64 bits) sur la [page des versions](https://github.com/BenribsLab/managecomp-rust/releases).
2. Décompressez l'archive où vous voulez.
3. Double-cliquez sur **ManageComp.exe**, à la racine du dossier (icône ManageComp). Rien d'autre à installer : tout ce qu'il faut est dans l'archive. **ManageComp Organisation.exe** et **ManageComp Live.exe** lancent l'organisation seule et l'écran des pistes.

## ManageComp pour Linux et Raspberry Pi

| Archive | Pour |
|---|---|
| `linux-x64` | PC sous Linux (64 bits) |
| `linux-arm64` | Raspberry Pi 4 et 5 (Raspberry Pi OS 64 bits) |
| `linux-armhf` | Raspberry Pi sous Raspberry Pi OS 32 bits |

1. Téléchargez l'archive `.tar.gz` qui correspond à votre système sur la [page des versions](https://github.com/BenribsLab/managecomp-rust/releases), puis décompressez-la.
2. Dans le dossier, lancez `./installer.sh` : il vérifie les bibliothèques nécessaires (GTK 4), propose de les installer s'il en manque, puis ajoute ManageComp au menu des applications et sur le bureau. Le logiciel reste dans ce dossier.
3. `./installer.sh --desinstaller` retire les raccourcis.

> Les versions marquées « préversion » sont en cours de validation : merci de signaler tout écart avec BellePoule.

## Version en ligne

Ouvrez la [version en ligne](https://app.managecomp.org) et inscrivez-vous : un administrateur valide votre compte ([ManageComp en ligne](@/docs/en-ligne.md)). Les compétitions sont enregistrées sur le serveur et partagées par les organisateurs du club.

## Arbitres

Rien à installer : chaque arbitre scanne avec l'appareil photo de son téléphone le QR code que lui remet l'organisation, et la page de saisie s'ouvre. Voir [Arbitres sur téléphone](@/docs/arbitre.md).

## Code source

ManageComp est un logiciel libre (GNU GPL version 3 ou ultérieure). Le code source complet est disponible sur [GitHub](https://github.com/BenribsLab). Voir [Licence et crédits](@/docs/licence.md).
