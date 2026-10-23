+++
title = "Arbitres sur téléphone"
weight = 11
description = "ManageComp Arbitre : la saisie des matchs par les arbitres."
+++

**ManageComp Arbitre** est une page web : rien à installer, elle fonctionne sur iPhone comme sur Android. Le QR code ouvre l'adresse **arbitre.managecomp.org** (ou celle de votre propre serveur).

## Préparer les QR codes

Dans le logiciel : menu **Organisation › Arbitres sur téléphone…**. Indiquez l'adresse du serveur et la clé de votre club, puis imprimez les QR codes : un par arbitre, et un QR **administrateur** qui voit tous les matchs.

## Côté arbitre

1. Scanner son QR code avec l'appareil photo du téléphone.
2. La liste **Mes matchs** s'affiche : poules et matchs affectés, piste et heure. Elle se met à jour toute seule.

{{ figure(src="arbitre-liste.png", caption="La liste des matchs de l'arbitre (ici le QR administrateur, qui voit tout).", class="phone") }}
3. Ouvrir une poule, puis un match : chrono, touches, cartons (jaune, rouge, noir), puis **Envoyer**.

{{ figure(src="arbitre-feuille.png", caption="Une poule : le tableau se remplit au fil des matchs envoyés.", class="phone") }}

{{ figure(src="arbitre-match.png", caption="La saisie d'un match : chrono, touches, cartons.", class="phone") }}

Les saisies sont gardées sur le téléphone si le réseau coupe et renvoyées ensuite.

## Rencontres par équipes

Pour une rencontre par équipes, l'arbitre désigne l'équipe du haut et du bas, note les tireurs de chaque numéro, puis saisit les relais un par un ([Rencontres par équipes](@/docs/equipes.md)).

{{ figure(src="arbitre-relais.png", caption="Une rencontre par équipes : le relais en cours et les relais terminés.", class="phone") }}

## Écran de la piste

Pendant le match, le téléphone envoie le score et le chrono à l'écran de la piste ([ManageComp Live](@/docs/live.md)).
