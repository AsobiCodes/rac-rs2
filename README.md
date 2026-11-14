# RAC-RS 2

An unofficial PC port of **Ratchet & Clank 2 / Going Commando** (PlayStation 2, 2003), written in Rust. It runs the game's own code, built from **your own disc**. It is the sister project of RAC-RS (Ratchet & Clank 1) and shares its engine.

> **No game data is included.** This package contains no game code, graphics, sound, text or other game files.
> The setup reads your own copy of the disc and creates everything it needs on your PC. You need to own the game.

Project site: https://asobicodes.dev
Support the project (no pressure tho!): https://ko-fi.com/asobicodes

## Status

**v0.1 is the first, early release.** Its goal is a game you can play from start to end without crashes and without visual glitches. Expect rough edges; see [Known issues](#known-issues).

- Gameplay follows the original PS2 game: the game logic is the original code.
- Widescreen (the picture also fills ultrawide windows), keyboard and mouse with a Controls page (rebind keys, mouse buttons and controller buttons, stick dead zone), gamepads (XInput, DualShock 4, DualSense), and an optional modern camera (mouse look, Static and Smooth camera, Raised or Steady strafe aim).
- Optional modern graphics (set Lighting to Enhanced): ambient occlusion, global illumination, sun shadows and sun rays, enhanced water with reflections and waves, volumetric fog and clouds, fire and smoke, explosion fire for several weapons, light from flying rockets, heat shimmer above lava, fur on Ratchet, shiny metal, vegetation, draw distance, frame smoothing, brightness and contrast, a sharp HUD. The Classic settings keep the original look.
- Modern menu pages (Options, Load, Save, vendor, Weapons, Gadgets, Items, Quick Select, Help, Sound, Galactic Map, Ship Shack) with mouse support. The Classic screens stay available (Menu style row), and the mouse works on most of them too.
- Upscalers on Windows: AMD FSR and NVIDIA DLSS, with Frame Generation. Vulkan or DirectX 12.
- Updates: the game checks by itself whether a newer version is out, see [Updating](#updating). The optional [RAC-RS Launcher](#launcher) app installs, starts and updates the game for you.
- PAL / Europe disc only for now. Windows 10 / 11 and Linux (x86-64).

## How it works

When you run setup, it translates the PS2 code from your disc into native Rust ("static recompilation"), and that code runs on a PS2 CPU core written in Rust. Because the game logic is the original code, gameplay behaves like the PS2 version. Everything around it is native PC code written for this port: graphics, sound, movies, input, saving and the PC features.

## What you need

- **The game disc:** Ratchet & Clank 2 / Going Commando, **PAL / Europe** (serial SCES-51607). Make an `.iso` image of **your own** disc. Setup checks the image. Other versions (USA, Japan, re-releases) are not supported yet.
- **Windows 10 or 11, 64-bit**, with about **6 GB of free disk space** and an internet connection for the first setup. A graphics card with Vulkan or DirectX 12 support.

## Setup (Windows)

1. Unzip **rac-rs2-0.1.1.zip** anywhere (a folder without special characters in its path is safest).
2. Double-click **Setup.cmd** and pick your `.iso` file. A tick-box offers to install the **RAC-RS Launcher** as well (an optional app, see [Launcher](#launcher)).
3. Wait. Setup gets its own private copy of its tools into the `toolchain` folder (nothing is installed on your PC, no admin rights), checks the disc, generates the game code from it and builds the game. The first build takes about 15 minutes on a fast PC and up to about half an hour on a 4-core CPU.
4. Play: double-click **"Ratchet & Clank 2 (rac-rs)"** on your desktop, or **Play.cmd** in the game folder (Play.cmd also switches on the diagnostic record, see [Reporting a problem](#reporting-a-problem)). `Setup.cmd -Console` runs setup in a console window if something goes wrong.

**Already have RAC-RS (Ratchet & Clank 1) or another RAC-RS install on this PC?** Setup finds it and copies its tools instead of downloading them again (or point it there: `Setup.cmd -ToolchainFrom "<folder>"`). It only takes tools that run and have the exact version Setup needs; anything else is downloaded as usual. Your other install is left as it is.

**Linux:** Already have RAC-RS (Ratchet & Clank 1) or another RAC-RS install? setup.sh copies its tools instead of downloading them (or point it there: `sh setup.sh --toolchain-from <folder>`).

Your saves are kept in the `player` folder. Setup does not touch an installed RAC-RS (Ratchet & Clank 1): the two games live in separate folders and have separate shortcuts.

**Do not share or upload the generated files or the built game.** They contain code generated from the game disc. Everything setup creates stays inside this folder.

## Linux

In the unzipped folder:

```
sh setup.sh --iso "/path/to/disc.iso"   # downloads the tools, checks the disc, generates the game code, builds
./play.sh                               # starts the game
```

`setup.sh` downloads its own tools into the game folder, needs no `sudo` and no development packages, only a Vulkan driver and the usual desktop libraries (ALSA, udev, the Vulkan loader, the X11 window libraries); it tells you if one is missing. Linux support was tested under WSL (Ubuntu) with software Vulkan; real drivers and desktops are not tested yet, so please report what you find. The Linux build has no Windows-only options (Renderer, upscalers).

## Updating

When you start the game, it checks online whether a newer version of RAC-RS 2 is out. On Windows, if there is one, a window asks you before the game starts: click **Update now** to update (your saves and settings are kept), or **Not now** to just play. If you are offline, or there is no newer version, nothing shows and the game just starts. On Linux the game shows a notice only; to update, use the Launcher. The check only downloads a small version file from this project's GitHub page; nothing about you or your game is sent.

## Launcher

The **RAC-RS Launcher** ([AsobiCodes/rac-rs-launcher](https://github.com/AsobiCodes/rac-rs-launcher)) is an optional app, the easy way to use this game. Version 0.1.7 or newer knows Ratchet & Clank 2. It can install the game for you (Library > New install > Game: Ratchet & Clank 2; it downloads this release and runs Setup), start it, change the PC options in a Settings page, and update it. The Settings page lists only the options this game really has; Resolution, the Graphics Preset and the key bindings are set in the game's own Options menu. The Mods and Achievements tabs are greyed out for this game, because it has neither yet. The game works the same without the launcher. In Setup (Windows), the tick-box installs the launcher too, and the desktop shortcut then opens it.

## Controls and options

Keyboard and mouse work out of the box, and the game's button prompts name your keys. Change any key, mouse button or controller button in Pause > Options > Controls (the Show row switches between Keyboard and mouse and Controller). Controllers work too, also when plugged in after the game has started. **F5** quick-saves and **F9** quick-loads the whole game state (the game restarts into your newest quick save).

All PC options are in the game's Options menu (pause menu, or the title screen): the pages are Display (window or fullscreen, aspect ratio, render scale, Resolution, Renderer, Upscaler and quality, Frame Generation, FPS limit, Brightness and Contrast, HUD), Graphics (Preset Low / Medium / High, Lighting Classic / Enhanced and the effects above, each with its own levels), Camera and Controls. The Renderer and Frame Generation take effect after a restart.

## Known issues

- **PAL / Europe disc only.** The USA disc comes later.
- **No achievements yet** (planned for v0.2) and no mod loader yet.
- **Linux is new and only tested under WSL** (Ubuntu with software Vulkan): real graphics drivers and desktops, the Linux update notice and the launcher's Linux version with this game are not tried on a real desktop.
- **Speed profile (PGO):** on Windows, Setup builds the game guided by a recording of the game's hot code (`pgo\`). The recording was made shortly before the last fixes of this version, so the code changed since then gets no benefit from it yet; a fresh recording comes in a later version. `Setup.cmd -NoPgo` builds without it.
- **Updates are new.** The update check and the update itself were tested with a local test version, not yet with a real later release. If an update ever fails, download the new ZIP, unzip it over your game folder and run Setup again (saves and settings are kept).
- **Tools reuse** (Setup using the tools of another RAC-RS install) was tested with copies of an R&C2 install; with a real Ratchet & Clank 1 install it falls back to downloading if anything does not match.
- **Hitches.** The frame rate can dip for a moment right after arriving on a level, and during big explosions (for example the Zodiac burst). Some PCs also show a small stutter every half second that comes from other programs (hardware monitor or lighting tools), not from the game.
- **Textures:** in rare places an object (a crate, a robot) can show a garbled texture. If you see it, press Insert (see below) and tell us where.
- **Weapon ghost:** after switching weapons a faint copy of the earlier weapon can stay drawn near the hands or hip.
- **Mouse aiming in first person** (the L1 view) may not turn the view on some PCs.
- **Dobbo (the testing facility):** water flicker at the landing pad is possible with an upscaler on.
- Snow and sand footprints are not in. Some fires and ice that stand in levels have no effect. The Bolt weapons (Heavy Lancer, Blitz Gun) and the Visibomb have no light.
- The 60 Hz mode is hidden (it does not work reliably yet).
- Map names in other languages, and the mouse on the Classic Galactic Map's planet list and on Yes / No boxes other than Quit, still behave like the original.
- Some menu pictures may differ slightly from the original. The modern title menu has a plain dark background, and the modern Ship Shack page shows no 3D preview of the ship part.
- Volumetric clouds (off by default) cost a lot of speed on some planets, and their look is not final.

## Reporting a problem

Open an issue on the project's GitHub page and attach what is in the game folder's `player` folder (everything stays on your PC; the game sends nothing anywhere):

- `player\log.txt` (the previous run is `player\log.prev.txt`), and `player\crash.log` if the game stopped.
- The **diagnostic record**: when you start the game with **Play.cmd** in the game folder (the desktop shortcut starts the game without it, so it writes no record), every session writes a small folder `player\diag\<date_time>\` with `system.txt` (your PC, graphics card, display and options), `perf.csv` (one line per second: frame rate, slowest frame, which part of the frame took the time), `spikes.txt` (every picture that took too long) and `marks.txt`. Zip the newest folder and attach it. The newest 10 sessions are kept, at most 500 MB. To switch the record off, delete the line `set "RAC_DIAG=1"` in `Play.cmd`.
- **Press Insert** at the moment something looks wrong or stutters: it writes a mark into the record and saves a picture (`mark_1.png`, ...). Then say roughly where you were.

## Legal

This is a fan project for preservation and education. It is **not affiliated with, endorsed by or sponsored by Sony Interactive Entertainment or Insomniac Games**. "Ratchet & Clank" is a trademark of Sony Interactive Entertainment LLC. All game content belongs to its owners.

NVIDIA, RTX and DLSS are trademarks and/or registered trademarks of NVIDIA Corporation in the U.S. and other countries. AMD, Radeon and FidelityFX are trademarks of Advanced Micro Devices, Inc. Windows and DirectX are trademarks of the Microsoft group of companies. This project is not affiliated with, endorsed by or sponsored by NVIDIA, AMD or Microsoft.

This package contains only original source code and tools. It contains no copyrighted game data.

## License

GPL-3.0-or-later, see [LICENSE](LICENSE). Credits for third-party work are in [NOTICE.md](NOTICE.md). The AMD and NVIDIA upscaler files are not in this package: Setup downloads them from the RAC-RS (Ratchet & Clank 1) v0.5.1 release (or copies them from another RAC-RS install on your PC), and their licence texts are in `third_party/licenses/`. The NVIDIA DLSS files are not under the GPL: they come under NVIDIA's own licence, and an additional permission under section 7 of the GPL allows RAC-RS to be shipped and run with them (see [NOTICE.md](NOTICE.md)).
