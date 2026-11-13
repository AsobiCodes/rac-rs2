//! [PC] The window side of R&C2's modern save-slot pages (lane ld; game side `modern_load.rs`, builder `modern_load_page.rs`): the
//! display-frame draw (the page over the title or the world; the retail overlay picture is left out except ONE crop of it, the
//! selected save's level picture, drawn into the hole under the cards) and the mouse while the page is up. A hover does nothing
//! (B15, as R&C1), a click on the card retail's cursor is on takes it (Cross: load, or save into it), a click on
//! another card selects it (R&C1: the second click takes it), the extra row is Circle (Continue without saving), a right click is
//! Triangle. The clicked or hovered card shows selected at once (`modern_load::shown`). Retail's box over a Save page (overwrite?,
//! saving) is a dialog over the cards: its buttons, Enter / Esc and a right click press Circle (Yes) / Triangle (No); no card or
//! hint acts under it. Keys and the pad otherwise reach the game as always.
use crate::modern_load::{self as ml, CIRCLE_BIT, CROSS_BIT, TRIANGLE_BIT};
use crate::modern_load_page as page;
use rac_engine::overlay::Quad;
use rac_engine::ui::font::FontMeasure;
use rac_engine::ui::kit::data::HintKind;
use rac_engine::ui::kit::page::{PageHit, PageLayout, PageScreen};
use rac_engine::ui::kit::{page_draw, Scale};
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::Gpu;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

struct Shown {
	screen: PageScreen,
	layout: PageLayout,
	win: (u32, u32),
	/// The card drawn selected, the cards and whether the extra row is there.
	sel: usize,
	/// Retail's own cursor card (B15: a press goes out only when this is the clicked card).
	cur: usize,
	cards: usize,
	extra: bool,
	/// The card the mouse last hovered (a hover asks the game once per card) and the card a click or hover aims at (`ml::shown`).
	hover: Option<usize>,
	aim: Option<(usize, u64)>,
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Shown>> {
	SHOWN.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> u64 {
	static T0: OnceLock<Instant> = OnceLock::new();
	T0.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Not drawn this frame (F1, load frames, a page above it): forget the window state, so `event` swallows no clicks.
pub(super) fn clear() {
	*lock() = None;
}

/// The GS fields done of the newest display frame that drew the world (`world_seen`); MAX = never.
static WORLD_AT: AtomicU64 = AtomicU64::new(u64::MAX);

/// Window thread, every display frame (`done` = the GS fields done of the picture shown): whether the pause page may draw
/// (`ml::world_step`): the world in this field's picture or one a few fields older. Retail draws its Load page without the world on
/// some fields while it loads a save (runs\ld6_pzlcl 454, 456): the page stays up over them instead of retail's classic picture.
pub(super) fn world_seen(world: bool, done: u64) -> bool {
	let last = Some(WORLD_AT.load(Ordering::Relaxed)).filter(|&at| at != u64::MAX);
	let (last, ok) = ml::world_step(last, world, done);
	WORLD_AT.store(last.unwrap_or(u64::MAX), Ordering::Relaxed);
	ok
}

/// Draws the page when the game thread read a slot page this field (at the title, or in the pause with the world drawn now or a
/// few frames ago: `world_seen`); false (and nothing drawn) otherwise.
pub(super) fn draw(g: &mut Gpu, world: bool) -> bool {
	let Some(f) = ml::facts().filter(|f| f.title || world) else {
		*lock() = None;
		return false;
	};
	let mut s = lock();
	let (hover, aim) = s.as_ref().map_or((None, None), |s| (s.hover, s.aim));
	let (sel, aim) = ml::shown(aim, f.cur, now_ms());
	let built = page::build(&f, sel, super::screens::look());
	let (layout, prims) = page_draw::frame(&built.screen, &FontMeasure);
	let pic = page::picture(&f, layout.panel.as_ref().map(|p| p.rect)).filter(|_| f.fade <= 0.0);
	// Valid for the next draw only: every frame while the page is up. Retail's fade after a title load goes over all of it.
	g.ui_gpu.set_scene(page::with_fade(page::with_picture(prims, pic.as_ref()), f.fade));
	g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	if let Some((hole, _)) = &pic {
		// The HUD list's pixel space is the render size (screens::hud_size), not the window: right with DLSS / FSR / Render scale.
		let hs = super::screens::hud_size(g);
		let r = Scale::fit(hs.0, hs.1).to_window(*hole);
		let uv = page::uv();
		g.hud.push(Quad { tex: RETAIL_OV_TEX, rect: [r.x, r.y, r.w, r.h], uv, colour: [1.0; 4], angle: 0.0, corners: None });
		if g.has_retail_hd() {
			g.hud.push(Quad { tex: RETAIL_HD_TEX, rect: [r.x, r.y, r.w, r.h], uv, colour: [1.0; 4], angle: 0.0, corners: None });
		}
	}
	*s = Some(Shown { screen: built.screen, layout, win: g.win_size(), sel, cur: f.cur, cards: built.cards, extra: built.extra, hover, aim });
	true
}

/// Aim at card `i`: drawn selected at once, retail's cursor walks there, then `press`.
fn aim_at(s: &mut Shown, i: usize, press: u16) {
	if i != s.sel {
		s.aim = Some((i, now_ms()));
		s.sel = i;
	}
	ml::request(Some(i), press, false);
}

/// The window event while the page is up (`ui_pos` = the cursor). True = the page took it.
pub(super) fn event(ui_pos: Option<(f32, f32)>, event: &WindowEvent) -> bool {
	let mut g = lock();
	let Some(s) = g.as_mut() else { return false };
	let hit = |s: &Shown, x: f32, y: f32| {
		let (dx, dy) = Scale::fit(s.win.0, s.win.1).to_design(x, y);
		s.layout.hit(&s.screen, dx, dy)
	};
	match event {
		WindowEvent::CursorMoved { position, .. } => {
			// [b45] B15: a hover never moves retail's cursor (R&C1: the cards have no hover target); only a click steps it.
			s.hover = match hit(s, position.x as f32, position.y as f32) {
				PageHit::Item(i) if i < s.cards => Some(i),
				_ => None,
			};
			false
		}
		WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
			let Some((x, y)) = ui_pos else { return true };
			let asks = s.screen.dialog.as_ref().is_some_and(|d| !d.buttons.is_empty());
			match (button, hit(s, x, y)) {
				// Retail's box: its buttons are Circle (Yes) and Triangle (No); a right click is No. Nothing else acts under it.
				(MouseButton::Left, PageHit::Button(k)) if asks => ml::request(None, if k == 0 { CIRCLE_BIT } else { TRIANGLE_BIT }, true),
				(MouseButton::Right, _) if asks => ml::request(None, TRIANGLE_BIT, true),
				_ if s.screen.dialog.is_some() => {}
				(MouseButton::Left, PageHit::Item(i)) if i < s.cards => {
					// [b45] B15: Cross only on the card RETAIL's cursor is on (`s.cur`, not the card drawn selected): click 1 walks the
					// cursor there, click 2 takes it; a click on the card the walk is still heading to does nothing.
					if ml::click_acts(i, s.cur, s.aim) {
						let press = ml::click_press(i, s.cur);
						aim_at(s, i, press);
					}
				}
				(MouseButton::Left, PageHit::Item(i)) if i == s.cards && s.extra => ml::request(None, CIRCLE_BIT, false),
				(MouseButton::Left, PageHit::Hint(HintKind::Change)) => ml::request(Some(s.sel), CROSS_BIT, false),
				(MouseButton::Left, PageHit::Hint(HintKind::Back | HintKind::Peek)) | (MouseButton::Right, _) => ml::request(None, TRIANGLE_BIT, false),
				_ => {}
			}
			true // a click on the page never reaches the game (the left button is the game's Circle)
		}
		// Retail's question: Enter = Yes (Circle), Esc / Backspace = No (Triangle), as R&C1's Overwrite box and the title's Quit. A key
		// held from before the box opened (its repeats) never answers it.
		WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat && s.screen.dialog.as_ref().is_some_and(|d| !d.buttons.is_empty()) => {
			match event.physical_key {
				PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => ml::request(None, CIRCLE_BIT, true),
				PhysicalKey::Code(KeyCode::Escape | KeyCode::Backspace) => ml::request(None, TRIANGLE_BIT, true),
				_ => return false,
			}
			true
		}
		_ => false,
	}
}
