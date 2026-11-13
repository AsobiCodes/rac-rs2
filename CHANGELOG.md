# Changelog

## v0.1.0 (2026-11-14)

First public release of RAC-RS 2: an unofficial PC port of **Ratchet & Clank 2 / Going Commando** (PlayStation 2, 2003), built on your PC from your own PAL / Europe disc (SCES-51607). No game data is included. v0.1 is an early release: the goal is a game you can play from start to end without visual glitches.

### Graphics (Options > Graphics; most need Lighting: Enhanced)

- New: ambient occlusion, global illumination, sun shadows and sun rays, each with levels.
- New: water with reflections and moving waves; volumetric fog that follows the place you are in; volumetric clouds on the sky planets (off by default).
- New: fire and smoke; explosion fire for the Gravity Bomb, Mini-Nuke, Bomb Glove, Minirocket Tube, Seeker Gun and the Hoverbomb and Tetrabomb mines; light from flying rockets; heat shimmer above lava.
- New: fur on Ratchet (not on his armor plates), shiny metal (Off / Low / Medium / High, with a reflections slider), draw distance, vegetation (Off / Moderate / Lush), the stars in the ship flights, Culling On / Off.
- New: smooth texture detail, Brightness and Contrast, Graphics Preset (Low / Medium / High).

### Screen and speed

- New: Resolution, window or fullscreen, aspect ratio (4:3 to 32:9, the picture fills ultrawide windows), render scale, frame limit, frame smoothing.
- New: AMD FSR and NVIDIA DLSS upscalers and Frame Generation (Graphics > Resolution; Windows).
- New: Vulkan or DirectX 12 renderer (after a restart); works with ReShade.
- New: sharp HUD, HUD corner layout and HUD size.
- Faster: smaller hitches in busy scenes and while objects change shape; explosions cost less time.

### Menus

- New: modern Options, Load, Save, vendor (with Buy all ammo), Weapons, Gadgets, Items, Quick Select, Help, Sound, Galactic Map (with a picture of each planet) and Ship Shack, all with the mouse. Menu style: Classic brings back the original screens; the mouse works on most of them too.
- New: the language is remembered; movie subtitles switch; Skip intro; Start skips cutscenes.

### Keyboard, mouse and controller

- New: full keyboard and mouse play: mouse look (also in first person and ship fights), strafe aiming Raised or Steady, Quick Select by mouse, mouse wheel directions as buttons.
- New: Controls page to rebind keys, mouse buttons and controller buttons, and a stick dead zone setting. Prompts in the game name your bound key, or show PlayStation / letter / Switch button pictures.
- New: mouse up/down and left/right invert on their own.
- New: controllers (XInput, DualShock 4, DualSense), also plugged in while the game runs.
- New: modern camera (Static and Smooth), camera speed and mouse speed.

### Other

- New: F5 quick save and F9 quick load; debug mode (level warp, free camera, ammo, god mode, bolts).
- New: a message and a line in `player\crash.log` if the game cannot start or stops.
- New: a diagnostic record for bug reports: `Play.cmd` writes one small folder per session to `player\diag\` (PC, options, one line per second, every long frame); press Insert to mark a moment and save a picture.
- New: Linux (x86-64) setup (`setup.sh`), tested under WSL only.
- New: Setup reuses the tools (Rust, Python, assembler, upscaler files) of another RAC-RS install on the same PC instead of downloading them again; `Setup.cmd -ToolchainFrom "<folder>"` points it at one.
- New: automatic update check. On Windows an update window can appear before the game starts (Update now / Not now); on Linux a notice points to the Launcher. It only downloads a small version file from this project's GitHub page.
- New: the RAC-RS Launcher (version 0.1.7 or newer) installs, starts, sets up and updates this game; Setup can install it with a tick-box.

### Known limitations

- PAL / Europe disc only. The USA disc, 60 Hz mode, achievements and the mod loader come later.
- The speed profile (PGO) that Setup builds with on Windows was recorded shortly before the last fixes of this version; the code changed since then gets no benefit from it yet.
- Linux was only tested under WSL; the Linux update notice and the launcher's Linux version with this game are not tried on a real desktop.
- The update check and the update itself were tested with a local test version only, not yet with a real later release.
- In the launcher, Resolution, the Graphics Preset and the key bindings are not on this game's Settings page (set them in the game's own menu), and the Mods and Achievements tabs are greyed out.
- Snow and sand footprints, some level fires and ice, and the light of the Bolt weapons and the Visibomb are not in yet.
- Small hitches right after arriving on a level and during big explosions; a faint copy of the earlier weapon after switching weapons; first-person mouse aiming may not work on some PCs; rare garbled textures. See the README.
- At Dobbo, water flicker is possible at the landing pad with an upscaler on.
- Volumetric clouds cost speed on some planets and their look is not final.
