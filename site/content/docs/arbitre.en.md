+++
title = "Referees on their phone"
slug = "referee"
weight = 11
description = "ManageComp Referee: bout entry by the referees."
+++

**ManageComp Referee** is a web page: nothing to install, it works on iPhone as well as Android. The QR code opens **arbitre.managecomp.org** (or your own server's address).

## Prepare the QR codes

In the software: **Organisation › Referees on phones…** menu. Enter the server address and your club key, then print the QR codes: one per referee, and an **administrator** QR code that sees every bout.

## On the referee's side

1. Scan the QR code with the phone's camera.
2. The **My bouts** list appears: assigned pools and bouts, piste and time. It updates by itself.

{{ figure(src="arbitre-liste.png", caption="The referee's bout list (here the administrator QR code, which sees everything).", class="phone") }}
3. Open a pool, then a bout: timer, touches, cards (yellow, red, black), then **Send**.

{{ figure(src="arbitre-feuille.png", caption="A pool: the grid fills in as bouts are sent.", class="phone") }}

{{ figure(src="arbitre-match.png", caption="Entering a bout: timer, touches, cards.", class="phone") }}

Entries are kept on the phone if the network drops and are sent again later.

## Team matches

For a team match, the referee sets the top and bottom teams, notes the fencer of each number, then enters the relays one by one ([Team events](@/docs/equipes.md)).

{{ figure(src="arbitre-relais.png", caption="A team match: the current relay and the finished relays.", class="phone") }}

## Piste screen

During the bout, the phone sends the score and timer to the piste's screen ([ManageComp Live](@/docs/live.md)).
