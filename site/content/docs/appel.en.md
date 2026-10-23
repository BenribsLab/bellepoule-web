+++
title = "Check-in"
slug = "check-in"
weight = 3
description = "Enter, import and check in fencers or teams."
+++

## Add fencers

- **Add**: the fencer form opens; the Enter key adds the fencer from any field.
- **Import**: FFF, CSV, FIE XML or ranking files.
- **Paste a copy**: duplicates fencers (and the members of a team).

Displayed columns are chosen from the list menu. Search and filters (All, Present, Absent) help to check in quickly.

## Mark who is present

{{ figure(src="appel.png", caption="Check-in: list of fencers, present and absent.") }}

Tick each fencer's presence box, or use **Present** / **Absent** to tick or untick everyone. Counters show how many are present and absent.

## Team event

{{ figure(src="equipes-appel.png", caption="Check-in for a team event: fencers per team and substitutes.") }}

Each fencer belongs to a team (*Team* column). In the check-in settings:

| Setting | Default | Purpose |
|---|---|---|
| Fencers per team | 3 | fencers of a team in a match |
| Substitutes | 1 | substitutes on top of the fencers |
| Minimum size | 3 | below it, the team is declared absent |

ManageComp checks:

- that a team does not exceed its fencers and substitutes: one more fencer is **refused** with a message;
- that a team that came has enough fencers present: otherwise a **warning** says "1 fencer missing".

## Close the check-in

**Next round** closes the check-in: present fencers go on to the next phase of the formula.
