+++
title = "Download"
template = "simple.html"
slug = "download"
description = "Download ManageComp for Windows or Linux (PC, Raspberry Pi), or use the online version."
+++

## ManageComp for Windows

1. Download the latest version (`.zip` archive for 64-bit Windows) from the [releases page](https://github.com/BenribsLab/managecomp-rust/releases).
2. Unzip the archive anywhere.
3. Double-click **ManageComp.exe**, at the root of the folder (ManageComp icon). Nothing else to install: everything needed is in the archive. **ManageComp Organisation.exe** and **ManageComp Live.exe** start the organisation alone and the piste screen.

## ManageComp for Linux and Raspberry Pi

| Archive | For |
|---|---|
| `linux-x64` | Linux PC (64-bit) |
| `linux-arm64` | Raspberry Pi 4 and 5 (64-bit Raspberry Pi OS) |
| `linux-armhf` | Raspberry Pi with 32-bit Raspberry Pi OS |

1. Download the `.tar.gz` archive matching your system from the [releases page](https://github.com/BenribsLab/managecomp-rust/releases), then extract it.
2. In the folder, run `./installer.sh`: it checks the required libraries (GTK 4), offers to install any missing ones, then adds ManageComp to the applications menu and the desktop. The software stays in this folder.
3. `./installer.sh --desinstaller` removes the shortcuts.

> Versions marked "pre-release" are being validated: please report any difference with BellePoule.

## Online version

Open the [online version](https://app.managecomp.org) and sign up: an administrator approves your account ([ManageComp online](@/docs/en-ligne.md)). Competitions are stored on the server and shared by the club's organisers.

## Referees

Nothing to install: each referee scans, with the phone's camera, the QR code given by the organisers, and the entry page opens. See [Referees on their phone](@/docs/arbitre.md).

## Source code

ManageComp is free software (GNU GPL version 3 or later). The complete source code is available on [GitHub](https://github.com/BenribsLab). See [License and credits](@/docs/licence.md).
