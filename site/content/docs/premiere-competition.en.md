+++
title = "First competition"
slug = "first-competition"
weight = 2
description = "Create a competition and set it up."
+++

## Create the competition

{{ figure(src="accueil.png", caption="The home screen: new competition, youth training, recent competitions.") }}

On the home screen, choose:

- **New competition** for an individual or team event;
- **New youth training** for an event in level groups ([Youth training](@/docs/entrainement-jeunes.md));
- **Open a competition** to resume a `.cotcot` file (or import a FIE, ZIP or AskFred file).

**New competition** first asks how to create it: from an **FFE zip** file (extranet), an **XML** file, an **AskFred** file (`.frd`, one competition per event), or an **empty competition**. The online version offers the same choices.

{{ figure(src="nouvelle-competition.png", caption="New competition: choosing how to create it.") }}

The properties window opens:

| Field | Purpose |
|---|---|
| Name, organiser, place | headers of printouts and publications |
| Weapon, gender, category | competition rules, suggested referees |
| Date | printouts, rankings |
| Team event | choose it before entering fencers: it cannot be changed afterwards |
| Colour | identifies the competition in the organisation and on screens |
| Scores: automatic filling | **Forbid** by default. **Allow** adds an "Automatic filling" button to pools and tables that draws random scores: to test a formula without entering results |

## Save

{{ figure(src="proprietes.png", caption="Competition properties and formula.") }}

The **Save** button (or `Ctrl+S`) saves the `.cotcot` file. The button shows "Saved" while nothing has changed. When closing, ManageComp only asks if the latest version has not been saved.

Recent competitions appear as thumbnails on the home screen. The **✕** of a thumbnail removes it from the list, after confirmation: the file is not deleted and still opens with **Open a competition**.

In the online version, the **✕** of a card removes the competition from the home page without deleting it. The **Competitions removed from the home page** link, at the bottom of the page, lists them: **Restore** puts one back, the bin deletes it for good.

## Several competitions

Several competitions can be open at once, each in its own tab. The [organisation](@/docs/organisation.md) (pistes and referees) is shared by all of them.
