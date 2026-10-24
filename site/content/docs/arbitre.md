+++
title = "Arbitres sur téléphone"
weight = 11
description = "ManageComp Arbitre : la saisie des matchs par les arbitres, le matériel, les absences et l'appel du Directoire technique."
+++

**ManageComp Arbitre** est une page web : rien à installer, elle fonctionne sur iPhone comme sur Android. Le QR code ouvre l'adresse **arbitre.managecomp.org** (ou celle de votre propre serveur).

## Deux sortes de QR codes

| QR code | Où | Accès |
|---|---|---|
| **Personnel** | feuille personnelle remise à chaque arbitre | direct, sans mot de passe : ses poules et ses matchs |
| **De la feuille** | feuille de poule et feuille de match imprimées | uniquement avec le **code PIN** d'un arbitre affecté à cette feuille |

Une feuille de poule traîne sur une table : n'importe qui peut en scanner le QR code. Sans le PIN d'un arbitre de la poule, il ne donne accès à rien.

## Préparer les arbitres

- **Logiciel** : menu **Organisation › Arbitres sur téléphone…**. Indiquez l'adresse du serveur et la clé de votre club, puis enregistrez.
- **Version en ligne** : page **Organisation**, bouton **📱 Arbitres sur téléphone** (affiché quand « Gérer les arbitres » est coché).

Chaque arbitre a un **QR code personnel** et un **code PIN** à 4 chiffres. À imprimer :

- **Feuilles personnelles** : une page par arbitre, avec son QR code personnel, son PIN et le mode d'emploi. À remettre en main propre.
- **Liste des PIN** : pour l'organisation, confidentielle.
- **QR code administrateur** : voit tous les matchs, sans PIN. À garder à la table d'arbitrage.

{{ figure(src="arbitre-codes.png", caption="Arbitres sur téléphone (version en ligne) : PIN, téléphones connectés, feuilles personnelles.") }}

{{ figure(src="arbitre-personnelle.png", caption="La feuille personnelle d'un arbitre : QR code personnel et code PIN.") }}

Si un PIN a circulé, **Nouveau code** donne à l'arbitre un nouveau QR code personnel et un nouveau PIN : les anciens ne marchent plus et ses téléphones doivent se reconnecter. Remettez-lui sa nouvelle feuille. Dans la version en ligne, **Déconnecter** oublie les téléphones d'un arbitre sans changer son code.

## Côté arbitre

1. Scanner avec l'appareil photo du téléphone son QR code personnel, ou le QR code de la feuille de poule ou de match.
2. Avec le QR code d'une feuille, entrer son **code PIN**. Il n'est demandé **qu'une fois pour la compétition** : le téléphone reste reconnu 3 jours (la journée ou le week-end).
{{ figure(src="arbitre-pin.png", caption="QR code d'une feuille : le code PIN de l'arbitre est demandé.", class="phone") }}

3. La liste **Mes matchs** s'affiche : poules et matchs affectés, piste et heure. Elle se met à jour toute seule.

{{ figure(src="arbitre-liste.png", caption="La liste des matchs de l'arbitre (ici le QR administrateur, qui voit tout).", class="phone") }}

4. Ouvrir une poule, puis un match : chrono, touches, cartons (jaune, rouge, noir), puis **Envoyer**.

{{ figure(src="arbitre-feuille.png", caption="Une poule : le tableau se remplit au fil des matchs envoyés.", class="phone") }}

{{ figure(src="arbitre-match.png", caption="La saisie d'un match : chrono, touches, cartons.", class="phone") }}

La grille de la poule tient dans la largeur du téléphone **en mode portrait** : noms courts (« DUPONT J. »), texte plus petit à partir de 8 tireurs. Au-delà de 9 tireurs, elle défile de côté, les noms restant fixés à gauche.

Les saisies sont gardées sur le téléphone si le réseau coupe et renvoyées ensuite.

## Matériel vérifié

Avant la poule, l'arbitre vérifie le matériel de chaque tireur. **Touchez le nom d'un tireur** dans la grille : une fenêtre affiche son nom en entier, son club et l'interrupteur **Matériel vérifié**. Une fois vérifié, le début de la case du nom est **souligné en vert**.

{{ figure(src="arbitre-tireur.png", caption="La fenêtre du tireur : matériel vérifié, signalement d'absence.", class="phone") }}

La vérification est enregistrée sur le serveur : elle reste après un rechargement de la page et un autre arbitre de la poule la voit aussi.

## Signaler une absence

Dans la même fenêtre, **Signaler l'absence du tireur** prévient l'organisation, par exemple : « Absence de Arthur Delahousse du club de Patata sur la piste 12 ». Le nom du tireur passe en rouge barré dans la grille. S'il arrive, **Le tireur est arrivé** retire le signalement.

## Appeler le Directoire technique

Le bouton rouge **📣 Appeler le Directoire technique**, tout en haut de chaque écran, prévient l'organisation : « Directoire technique appelé piste 12 par : Nathalie Sang ». Il passe au vert une fois l'appel envoyé.

## Côté organisation : les signalements

Les absences et les appels arrivent à l'organisation :

- une **puce rouge** sur **Organisation**, avec le nombre de signalements en cours ;
- un **son** : deux notes pour une absence, trois notes montantes pour l'appel du Directoire technique ;
- la liste des signalements (poule, compétition, arbitre, heure), avec le bouton **Traité** : en haut de la page Organisation dans la version en ligne, dans la fenêtre **Arbitres sur téléphone** du logiciel (qui affiche aussi un message à chaque nouveau signalement).

{{ figure(src="organisation-signalements.png", caption="Les signalements en haut de la page Organisation, et la puce rouge du menu.") }}

## PIN faux : blocage de la feuille

Après **20 PIN faux** sur une feuille, elle est suspendue **5 minutes** ; après **20 de plus**, elle est **bloquée**. L'organisation en est avertie et la débloque : bouton **Débloquer**, dans le bandeau de la page Organisation (version en ligne) ou dans la fenêtre Arbitres sur téléphone (logiciel).

## Rencontres par équipes

Pour une rencontre par équipes, l'arbitre désigne l'équipe du haut et du bas, note les tireurs de chaque numéro, puis saisit les relais un par un ([Rencontres par équipes](@/docs/equipes.md)).

{{ figure(src="arbitre-relais.png", caption="Une rencontre par équipes : le relais en cours et les relais terminés.", class="phone") }}

## Écran de la piste

Pendant le match, le téléphone envoie le score et le chrono à l'écran de la piste ([ManageComp Live](@/docs/live.md)).
