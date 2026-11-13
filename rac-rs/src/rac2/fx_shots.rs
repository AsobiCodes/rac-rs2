//! [PC] [b47 X1] The player's shots for Fire and the weapon lights (R&C1 rac1/retail/blast_gate.rs and rac1/weapon_lights.rs): which moby
//! records of a field are the player's explosive shots (engine `fxpool::ShotZone`, `PoolFx::blast_gate`: an explosion sprite of a Blast
//! row becomes the simulated fire burst and ground fire only when it is born where one of them VANISHED a moment ago, so an enemy's own
//! death explosion stays retail) and which of them glow (engine `shot_rows::ShotPoint`, rows `ShotSrc::Point` in fx_rows.rs `SHOTS`).
//! The class id is the record's u16 at +0xaa (rac2/NOTES.md, the same as RAC2_SCENELOG / RAC2_MOBYLOG print).
//! Classes [C: find-out runs of build 46, RAC2_MOBYLOG, `scratch_rc2_b42\b47\x1\runs\`]: Gravity Bomb 0xce4 (fg1 0012: born at the hero
//! the field after Circle, flies ~10 m, the record goes the field the blast's pool sprites appear), Minirocket Tube rocket 0x5da (fg1 0359:
//! born 1.2-1.5 m from the hero after Circle, flies up to 69 m and goes). [b47 X1c] Seeker Gun missile 0xd01 (fg1 0316: born 0.8-0.9 m
//! from the hero the field after Circle, hovers, homes in and goes where the enemy's type-97 explosion appears; trail = orange type-23
//! puffs), Bomb Glove bomb 0x79 (0316: born 1.2 m, lands ~10 m away, its blast = orange type 11 + type 4 like the Gravity Bomb's),
//! Mini-Nuke 0xc07 (0359, the Gravity Bomb's upgrade: born 1.2 m, goes at 11.6 m, blast = orange type 11 + type 4 centred where it went);
//! runs `scratch_rc2_b42\b47\x1\runs\wp_*`. [b48 Y1, runs `b48\y1\runs\D_*`, fg1 0316 with the weapon owned] The Megarocket Cannon
//! (item 69) fires the Minirocket's class 0x5da (blast = type 97 in 4 colours at the vanish place: that row exists); Hoverbomb Gun and
//! Tetrabomb Gun mines = class 0x5d8 (1 / 5 records per shot, each goes where a type-97 blast of 4 colours is born within 3 m).
//! Not in the list (b48\handoff_Y1.md): Bouncer 0xe64 (rolls to rest 16 m away, goes with no pool blast), Spiderbot (the record is the
//! remote bot itself, no blast), RYNO II 0xed7 (blast = BLUE type 97: the retail plasma look stays), Lava Gun (no moby), Visibomb.
use crate::scene::Moby;
use rac_engine::fxpool::ShotZone;
use rac_engine::shot_rows::ShotPoint;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Gravity Bomb (item 42) in flight.
pub const GRAVITY_BOMB: u16 = 0xce4;
/// Minirocket Tube (item 27) rocket.
pub const MINIROCKET: u16 = 0x5da;
/// [b47 X1c] Seeker Gun (item 24) missile.
pub const SEEKER: u16 = 0xd01;
/// [b47 X1c] Bomb Glove (item 12) bomb.
pub const BOMB_GLOVE: u16 = 0x79;
/// [b47 X1c] Mini-Nuke (item 71, the Gravity Bomb's upgrade) in flight.
pub const MINI_NUKE: u16 = 0xc07;
/// [b48 Y1] Hoverbomb Gun (item 25) mine and Tetrabomb Gun (item 70, its upgrade) mines: one class, 1 resp. 5 records per shot.
pub const HOVERBOMB: u16 = 0x5d8;
/// [b49 Z1] RYNO II (item 44) rocket: class 0xed7 [C b48 Y1b: 2 shots, born at the hero, go at 11-15 m]. Light only, no zone: its blast
/// (blue type 97, 00 20 ff / 40 7f ff) stays the retail plasma look.
pub const RYNO: u16 = 0xed7;
/// Colour of the RYNO II rocket's light: the blue of its blast sprites.
const RYNO_RGB: [f32; 3] = [0.25, 0.5, 1.0];
/// Light tag of the rockets (`ShotSrc::Point`, fx_rows.rs `SHOTS`).
pub const TAG_ROCKET: u8 = 1;

/// (class, reach in m, light tag): how far from where a shot vanished an explosion sprite may be born and still be its explosion (R&C1's
/// values: a bomb 3.5, a rocket 3.0; the shot was last seen up to a field before its blast) and the light row it feeds, if any. The Gravity
/// Bomb gets no light (R&C1 lights only its rockets and bolts; the bomb's glow is its own sprite). [b47 X1c] The Seeker missile is a
/// rocket (orange exhaust: the rocket light), the Bomb Glove's bomb and the Mini-Nuke are bombs.
const SHOTS: &[(u16, f32, Option<u8>)] = &[
	(GRAVITY_BOMB, 3.5, None),
	(MINIROCKET, 3.0, Some(TAG_ROCKET)),
	(SEEKER, 3.0, Some(TAG_ROCKET)),
	(BOMB_GLOVE, 3.5, None),
	(MINI_NUKE, 3.5, None),
	(HOVERBOMB, 3.5, None),
];

/// The player's shots of one field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shots {
	pub zones: Vec<ShotZone>,
	pub points: Vec<ShotPoint>,
}

/// The shots among `mobies` (record address, class id, place).
pub fn of(mobies: impl Iterator<Item = (u32, u16, [f32; 3])>) -> Shots {
	let mut s = Shots::default();
	for (addr, class, pos) in mobies {
		if class == RYNO {
			s.points.push(ShotPoint { tag: TAG_ROCKET, id: addr, pos, rgb: Some(RYNO_RGB) });
			continue;
		}
		let Some(&(_, reach, tag)) = SHOTS.iter().find(|r| r.0 == class) else { continue };
		s.zones.push(ShotZone { id: addr, kind: class as u32, pos, reach });
		if let Some(tag) = tag {
			s.points.push(ShotPoint { tag, id: addr, pos, rgb: None });
		}
	}
	s
}

fn class_of(ram: &[u8], addr: u32) -> u16 {
	let a = (addr & 0x01ff_ffff) as usize + 0xaa;
	ram.get(a..a + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

/// The last fields' shots by field (the redraw shows the scene of one of them, fx_movers.rs `Feed::frames`).
static FIELDS: Mutex<VecDeque<(u64, Arc<Shots>)>> = Mutex::new(VecDeque::new());
const KEEP: usize = 8;

/// After a field (host/game.rs, with the scene's mobies): keeps this field's shots while Fire / the weapon lights want the pool.
pub fn note(field: u64, ram: &[u8], mobies: &[Moby]) {
	if !crate::fx_pool::wanted() {
		return;
	}
	let s = Arc::new(of(mobies.iter().map(|m| (m.addr, class_of(ram, m.addr), m.pos))));
	let mut q = FIELDS.lock().unwrap_or_else(|e| e.into_inner());
	q.retain(|e| e.0 < field);
	q.push_back((field, s));
	while q.len() > KEEP {
		q.pop_front();
	}
}

/// The shots of `field`; a field that was not noted takes the newest noted one before it, at most `KEEP` fields back ([b47 R6]: an
/// empty list there told the engine's gate that every live shot had vanished), else none.
pub fn of_field(field: u64) -> Arc<Shots> {
	pick(&FIELDS.lock().unwrap_or_else(|e| e.into_inner()), field)
}

fn pick(q: &VecDeque<(u64, Arc<Shots>)>, field: u64) -> Arc<Shots> {
	q.iter().rev().find(|e| e.0 <= field && field - e.0 <= KEEP as u64).map(|e| e.1.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bombs_and_rockets_are_zones_and_rockets_glow() {
		let s = of([(0x1981f80, GRAVITY_BOMB, [1.0, 2.0, 3.0]), (0x1dd0640, MINIROCKET, [4.0, 5.0, 6.0]), (0x1dd0740, 0x70, [0.0; 3])].into_iter());
		assert_eq!(s.zones.len(), 2);
		assert_eq!((s.zones[0].id, s.zones[0].kind, s.zones[0].reach), (0x1981f80, 0xce4, 3.5));
		assert_eq!(s.points, vec![ShotPoint { tag: TAG_ROCKET, id: 0x1dd0640, pos: [4.0, 5.0, 6.0], rgb: None }]);
		let mut ram = vec![0u8; 0x200];
		ram[0x1aa..0x1ac].copy_from_slice(&0xce4u16.to_le_bytes());
		assert_eq!(class_of(&ram, 0x8000_0100), 0xce4);
		assert_eq!(class_of(&ram, 0x0100_0000), 0, "out of the slice");
	}

	#[test]
	fn a_field_not_noted_takes_the_newest_noted_one_before_it() {
		let shots = |id: u32| Arc::new(of([(id, MINIROCKET, [0.0; 3])].into_iter()));
		let q: VecDeque<(u64, Arc<Shots>)> = [(10, shots(1)), (12, shots(2))].into_iter().collect();
		assert_eq!(pick(&q, 12).zones[0].id, 2);
		assert_eq!(pick(&q, 11).zones[0].id, 1, "not noted: the field before");
		assert_eq!(pick(&q, 15).zones[0].id, 2);
		assert!(pick(&q, 9).zones.is_empty() && pick(&q, 12 + KEEP as u64 + 1).zones.is_empty(), "before the first / too old");
	}

	#[test]
	fn hoverbomb_and_tetrabomb_mines_are_bomb_zones_without_light() {
		let s = of([(0x1aca040, HOVERBOMB, [1.0; 3]), (0x1aca240, 0x5d9, [2.0; 3])].into_iter());
		assert_eq!(s.zones.iter().map(|z| (z.kind, z.reach)).collect::<Vec<_>>(), vec![(0x5d8, 3.5)]);
		assert!(s.points.is_empty());
	}

	#[test]
	fn seeker_glove_and_nuke_shots() {
		let s = of([(0x1acdb40, SEEKER, [1.0; 3]), (0x1ac9b40, BOMB_GLOVE, [2.0; 3]), (0x1dcc040, MINI_NUKE, [3.0; 3])].into_iter());
		assert_eq!(s.zones.iter().map(|z| (z.kind, z.reach)).collect::<Vec<_>>(), vec![(0xd01, 3.0), (0x79, 3.5), (0xc07, 3.5)]);
		assert_eq!(s.points, vec![ShotPoint { tag: TAG_ROCKET, id: 0x1acdb40, pos: [1.0; 3], rgb: None }]);
	}

	#[test]
	fn ryno_rocket_glows_blue_without_a_zone() {
		let s = of([(0x1dd0640, RYNO, [1.0; 3])].into_iter());
		assert!(s.zones.is_empty());
		assert_eq!(s.points, vec![ShotPoint { tag: TAG_ROCKET, id: 0x1dd0640, pos: [1.0; 3], rgb: Some(RYNO_RGB) }]);
	}
}
