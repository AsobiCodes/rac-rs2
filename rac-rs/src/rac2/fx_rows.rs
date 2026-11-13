//! [PC] Which of R&C2's effect-pool sprites are fire, smoke or a weapon's light (engine `Pack::fire_rows`, `Pack::smoke`,
//! `Pack::shot_lights`; the sprites come from host/ov_fire.rs). Effect types (slot byte +0) by rac2/notes_fx_pool.md "T4 kinds"
//! [C: types seen in the recordings, their textures and colours; I: the per-type verdict]: fire Burn 4, embers 15, flames 21 / 44,
//! explosions 8 / 74 / 97; smoke 0x17 and 4 (grey puffs only: type 4 is also the blue plasma burn) and 48 (the steam column);
//! NOT 19 (a ribbon), 0x16 (the tracer), 2. The parameters are R&C1's (rac1/smoke_table.rs, rac1/fire_look.rs, rac1/weapon_lights.rs)
//! for the same kinds, so the look matches R&C1's approved one.
use rac_engine::fire_option::{FireKind, FireRow};
use rac_engine::shot_rows::{ShotRow, ShotSrc};
use rac_engine::smoke_rows::{SmokeRow, ANY_TEX};

const fn ty(ty: u32, kind: FireKind) -> FireRow {
	FireRow { key: ty, kind, dark: false }
}

/// Drawn as fire, never weighed for the fire light (R&C1's flying embers, same type 15).
const fn ember(ty: u32) -> FireRow {
	FireRow { key: ty, kind: FireKind::Flame, dark: true }
}

pub static FIRE: &[FireRow] = &[
	ty(4, FireKind::Burn),
	ember(15),
	ty(21, FireKind::Flame),
	ty(44, FireKind::Flame),
	ty(8, FireKind::Blast),
	ty(74, FireKind::Blast),
	ty(97, FireKind::Blast),
	// [b47 X1] The Gravity Bomb's blast is no type 8 / 74 / 97: it is 175 orange type-11 sprites (0xf4 0x6a 0x00, tex 0xa, life 21) with
	// 63 type-4 burns and 250 type-15 embers [C: RAM dump fg1 0012 field 70, run b47/x1/runs/gb]. Type 11 is also the magenta energy ball
	// (a light row, `SHOTS`): only its flame-coloured slots are handed as `BLAST_11` (`sprite_type`), and as a Blast row only those born
	// where a player's shot vanished take the fire burst (fx_shots.rs, engine `BlastFire` gate); every other one stays the retail sprite.
	ty(BLAST_11 as u32, FireKind::Blast),
];

/// [b47 X1] The engine's effect type for a flame-coloured type-11 slot (no R&C2 pool type: the bank's types end below 0x80, notes_fx_pool.md).
pub const BLAST_11: u8 = 0xf0;

/// [b47 X1] The effect type the engine gets for a slot (host/ov_fire.rs `step`): a flame-coloured type 11 is `BLAST_11`, the rest as is.
pub fn sprite_type(ty: u8, rgba: u32) -> u8 {
	if ty == 11 && flame_hue(rgba) {
		BLAST_11
	} else {
		ty
	}
}

/// `Pack::fire_rows`: the same rows in every level.
pub fn fire_rows(_level: usize) -> &'static [FireRow] {
	FIRE
}

pub static SMOKE: &[SmokeRow] = &[
	// R&C1's type 0x17 and 4 rows (grey only), and its type-6 column row for the steam column 48.
	SmokeRow { kind: 0x17, tex: ANY_TEX, grey_only: true, radius: 0.5, density: 1.0, rise: 1.0, billow: 0.35 },
	SmokeRow { kind: 0x04, tex: ANY_TEX, grey_only: true, radius: 0.5, density: 1.1, rise: 1.2, billow: 0.35 },
	SmokeRow { kind: 48, tex: ANY_TEX, grey_only: false, radius: 0.55, density: 0.9, rise: 1.6, billow: 0.45 },
];

const fn shot(ty: u8, rgb: [f32; 3]) -> ShotRow {
	// R&C1's Blaster bolt: reach 5 m, brightness 1, no flicker, gathered within 1.5 m, at most 8 lights.
	ShotRow { src: ShotSrc::Sprite(ty), rgb, reach: 5.0, gain: 1.0, spec: 1.0, flicker: 0.0, gather: 1.5, max: 8, with: None, hero_r: 0.0, follow: 0.0 }
}

/// Weapon lights from the shots' own sprites: 69 the blaster ball, 11 the energy ball, 91 the tracer sparks [I: notes_fx_pool.md;
/// per weapon table still open]. The colours are fallbacks: a light takes its hue from the sprites it gathers.
pub static SHOTS: &[ShotRow] = &[
	shot(69, [0.7, 0.95, 1.0]),
	shot(11, [1.0, 0.6, 1.0]),
	// [b47 X1] The flame-coloured type 11 handed as `BLAST_11`: the same light it gave as type 11 before (build 46).
	shot(BLAST_11, [1.0, 0.6, 1.0]),
	shot(91, [1.0, 0.6, 0.25]),
	// [b47 X1] The Minirocket Tube's rocket (fx_shots.rs points): R&C1's R.Y.N.O. rocket row (rac1/weapon_lights.rs): reach 7 m, 1.2, flicker 0.12.
	ShotRow { src: ShotSrc::Point(crate::fx_shots::TAG_ROCKET), rgb: [1.0, 0.6, 0.25], reach: 7.0, gain: 1.2, spec: 1.0, flicker: 0.12, gather: 0.0, max: 8, with: None, hero_r: 0.0, follow: 0.0 },
];

/// Is effect type `ty` one any row reads (host/ov_fire.rs keeps only these sprites)?
pub fn wanted(ty: u8) -> bool {
	FIRE.iter().any(|r| r.key == ty as u32) || SMOKE.iter().any(|r| r.kind == ty) || SHOTS.iter().any(|r| r.src == ShotSrc::Sprite(ty))
}

/// Texture byte (slot +2) of the weapon hit flashes that share a flame type with real fire (see `wanted_slot`).
const HIT_FLASH_TEX: u8 = 0x32;

/// A flame's colour (slot word +4, R low byte): red or green leads and blue is at most 0.6 of it. R&C1's explosion gate
/// (engine viewer/firefx.rs `blast_like`: blue <= 0.6 red) widened to green as `fire_option::is_hot` is.
fn flame_hue(rgba: u32) -> bool {
	let [r, g, b, _] = rgba.to_le_bytes().map(f32::from);
	let lead = r.max(g);
	lead > 0.0 && b <= 0.6 * lead
}

/// Is this slot one a row may take? The fire types 4 / 21 / 74 / 97 also draw the blue and magenta plasma with the same type and
/// texture [C: rac2/notes_fx_pool.md "T4 kinds", 39-47 % cool; the slot holds no spawner, only type + tex + colour], so a fire-row
/// type is kept only with a flame's colour (`flame_hue`) or, when Smoke also reads the type, a grey one (Smoke's `grey_only`).
/// R&C1 picks fire the same way: rows by type, then a colour gate (viewer/firefx.rs `flame_like` / `blast_like`).
pub fn wanted_slot(ty: u8, tex: u8, rgba: u32) -> bool {
	if !wanted(ty) {
		return false;
	}
	// [b46 W1] The Lancer's hit flash: type 44 with texture 0x32, yellow (0x80 0x70 0x20), two stacked slots of 525000 at the impact
	// point, life 3 -> 1 [C: RAM dumps of a Lancer run at fg1 0011 fields 8-10; whole bank: 4 slots in 0005 / 0011]. Real flames of
	// type 44 use texture 0x2f / 0xa / 0x31 (orange 0x80 0x40 0x00, size <= 52500 in 10 states of level 3), so a flame row never takes it.
	if ty == 44 && tex == HIT_FLASH_TEX {
		return false;
	}
	if !FIRE.iter().any(|r| r.key == ty as u32) || flame_hue(rgba) {
		return true;
	}
	let c = rgba.to_le_bytes().map(|b| b as f32 / 128.0);
	SMOKE.iter().any(|r| r.kind == ty && r.grey_only) && rac_engine::smoke_rows::saturation([c[0], c[1], c[2]]) <= rac_engine::smoke_rows::GREY_LIMIT
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rows_name_the_fire_smoke_and_shot_types_and_not_the_ribbon_or_tracer() {
		for t in [4u8, 15, 21, 44, 8, 74, 97, 0x17, 48, 69, 11, 91] {
			assert!(wanted(t), "{t}");
		}
		for t in [19u8, 0x16, 2] {
			assert!(!wanted(t), "{t}");
		}
		assert_eq!(rac_engine::fire_option::sprite_kind(fire_rows(3), 44), Some(FireKind::Flame));
		assert!(rac_engine::fire_option::sprite_dark(FIRE, 15));
		assert_eq!(rac_engine::smoke_rows::find(SMOKE, 4, 9), Some(1));
	}

	#[test]
	fn plasma_colours_of_the_fire_types_stay_retail_and_flames_and_grey_puffs_are_kept() {
		// Words are 0xAABBGGRR. Blue plasma of type 4 (notes: 3bff7f7f), a magenta flare, an orange flame, a grey puff.
		assert!(!wanted_slot(4, 0x2f, 0x3bff_7f7f), "blue plasma");
		assert!(!wanted_slot(21, 0x2f, 0x80ff_20ff), "magenta flare");
		assert!(!wanted_slot(74, 0x2f, 0x80ff_4040), "blue burst");
		assert!(wanted_slot(4, 0x2f, 0x8010_4080), "orange burn");
		assert!(wanted_slot(97, 0x2f, 0x8020_60ff), "orange spark");
		assert!(wanted_slot(4, 0x2f, 0x6070_7070), "grey puff of type 4 = Smoke's");
		assert!(!wanted_slot(44, 0x2f, 0x6070_7070), "grey of a fire-only type");
		assert!(wanted_slot(0x17, 0x2f, 0x73ff_ffff) && wanted_slot(69, 0x2f, 0x80ff_ff60), "smoke and shot types keep any colour");
		assert!(!wanted_slot(19, 0x2f, 0x8010_4080), "no row");
		// The Lancer's hit flash (type 44, texture 0x32, yellow) stays retail; the orange flame of the same type (texture 0x2f) is fire.
		assert!(!wanted_slot(44, 0x32, 0x1f20_7080) && wanted_slot(44, 0x2f, 0x0000_4080));
		// [b47 X1] The Gravity Bomb's orange type-11 blast sprites are Blast fire; the magenta energy ball of the same type is still taken
		// (its light row), the engine's blast colour gate keeps it retail.
		// A Blast row is watched (the burst and the ground fire start from it), not drawn as a sprite: `blast_type`, not `sprite_kind`.
		assert!(rac_engine::fire_option::blast_type(FIRE, BLAST_11, false) && !rac_engine::fire_option::blast_type(FIRE, 11, false));
		assert_eq!(rac_engine::fire_option::sprite_kind(FIRE, BLAST_11), None);
		assert_eq!(rac_engine::fire_option::sprite_kind(FIRE, 11), None);
		assert!(wanted_slot(11, 0x0a, 0x4c00_6af4) && wanted_slot(11, 0x0a, 0x80ff_60ff));
		assert_eq!((sprite_type(11, 0x4c00_6af4), sprite_type(11, 0x80ff_60ff), sprite_type(44, 0x0000_4080)), (BLAST_11, 11, 44));
	}
}
