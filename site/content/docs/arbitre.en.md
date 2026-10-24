+++
title = "Referees on their phone"
slug = "referee"
weight = 11
description = "ManageComp Referee: bout entry by the referees, equipment, absences and calling the technical directorate."
+++

**ManageComp Referee** is a web page: nothing to install, it works on iPhone as well as Android. The QR code opens **arbitre.managecomp.org** (or your own server's address).

## Two kinds of QR codes

| QR code | Where | Access |
|---|---|---|
| **Personal** | personal sheet given to each referee | direct, no password: the referee's pools and bouts |
| **Sheet** | printed pool sheet and match sheet | only with the **PIN code** of a referee assigned to that sheet |

A pool sheet lies around on a table: anyone can scan its QR code. Without the PIN of one of the pool's referees, it gives access to nothing.

## Prepare the referees

- **Software**: **Organisation › Referees on phones…** menu. Enter the server address and your club key, then save.
- **Online version**: **Organisation** page, **📱 Referees on phones** button (shown when "Manage referees" is ticked).

Each referee has a **personal QR code** and a 4-digit **PIN code**. To print:

- **Personal sheets**: one page per referee, with the personal QR code, the PIN and instructions. Hand them over in person.
- **PIN list**: for the organisers, confidential.
- **Administrator QR code**: sees every bout, no PIN. Keep it at the referees' desk.

{{ figure(src="arbitre-codes.png", caption="Referees on phones (online version): PINs, connected phones, personal sheets.") }}

{{ figure(src="arbitre-personnelle.png", caption="A referee's personal sheet: personal QR code and PIN code.") }}

If a PIN has leaked, **New code** gives the referee a new personal QR code and a new PIN: the old ones stop working and the referee's phones must reconnect. Give the referee the new sheet. In the online version, **Disconnect** forgets a referee's phones without changing the code.

## On the referee's side

1. Scan, with the phone's camera, the personal QR code, or the QR code of the pool or match sheet.
2. With a sheet's QR code, enter the **PIN code**. It is asked **only once for the competition**: the phone stays recognised for 3 days (the day or the weekend).
{{ figure(src="arbitre-pin.png", caption="A sheet's QR code: the referee's PIN code is asked.", class="phone") }}

3. The **My bouts** list appears: assigned pools and bouts, piste and time. It updates by itself.

{{ figure(src="arbitre-liste.png", caption="The referee's bout list (here the administrator QR code, which sees everything).", class="phone") }}

4. Open a pool, then a bout: timer, touches, cards (yellow, red, black), then **Send**.

{{ figure(src="arbitre-feuille.png", caption="A pool: the grid fills in as bouts are sent.", class="phone") }}

{{ figure(src="arbitre-match.png", caption="Entering a bout: timer, touches, cards.", class="phone") }}

The pool grid fits the phone's width **in portrait mode**: short names ("DUPONT J."), smaller text from 8 fencers. Beyond 9 fencers, it scrolls sideways with the names fixed on the left.

Entries are kept on the phone if the network drops and are sent again later.

## Equipment checked

Before the pool, the referee checks each fencer's equipment. **Tap a fencer's name** in the grid: a window shows the full name, the club and the **Equipment checked** switch. Once checked, the start of the name cell is **underlined in green**.

{{ figure(src="arbitre-tireur.png", caption="The fencer window: equipment checked, absence report.", class="phone") }}

The check is stored on the server: it remains after reloading the page, and another referee of the pool sees it too.

## Report an absence

In the same window, **Report the fencer's absence** alerts the organisers, for instance: "Absence de Arthur Delahousse du club de Patata sur la piste 12". The fencer's name is struck through in red in the grid. If the fencer arrives, **The fencer has arrived** withdraws the report.

## Call the technical directorate

The red **📣 Call the technical directorate** button, at the very top of every screen, alerts the organisers: "Directoire technique appelé piste 12 par : Nathalie Sang". It turns green once the call is sent.

## On the organisers' side: reports

Absences and calls reach the organisers as:

- a **red badge** on **Organisation**, with the number of open reports;
- a **sound**: two notes for an absence, three rising notes for a call to the technical directorate;
- the list of reports (pool, competition, referee, time), with a **Handled** button: at the top of the Organisation page in the online version, in the software's **Referees on phones** window (which also shows a message for each new report).

{{ figure(src="organisation-signalements.png", caption="Reports at the top of the Organisation page, and the red badge in the menu.") }}

## Wrong PINs: sheet blocked

After **20 wrong PINs** on a sheet, it is paused for **5 minutes**; after **20 more**, it is **blocked**. The organisers are warned and unblock it with **Unlock**, in the banner of the Organisation page (online version) or in the Referees on phones window (software).

## Team matches

For a team match, the referee sets the top and bottom teams, notes the fencer of each number, then enters the relays one by one ([Team events](@/docs/equipes.md)).

{{ figure(src="arbitre-relais.png", caption="A team match: the current relay and the finished relays.", class="phone") }}

## Piste screen

During the bout, the phone sends the score and timer to the piste's screen ([ManageComp Live](@/docs/live.md)).
