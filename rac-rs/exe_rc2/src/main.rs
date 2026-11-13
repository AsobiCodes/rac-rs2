// The R&C2 exe (rac2-play): the R&C2 pack on the engine (crate rac2, ../../src/rac2) plus the recompiled R&C2 retail code
// (crate ee-game-rc2, generated from the player's disc).

// Public build (cargo feature `windows_gui` of a later release tree): no console window next to the game.
#![cfg_attr(all(windows, feature = "windows_gui"), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
	rac2::set_register(ee_game_rc2::register);
	rac2::main()
}
