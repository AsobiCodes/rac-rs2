//! [PC] The window side of R&C2's modern Quick Select and Help pages (lane pages2; game sides `modern_qs.rs` / `modern_help.rs`,
//! builders `modern_qs_page.rs` / `modern_help_page.rs`): the display-frame draw (the page over the world; the retail overlay picture
//! is left out, except on Help one crop of it, the row's icon, drawn into the detail panel's hole) and the mouse while a page is up.
//! Quick Select: hover walks the game's grid cursor to the cell under the mouse, a left click on a cell equips it (Cross once the
//! cursor is there), a click on a ring slot or an arrow turns the ring (L1 / R1), a click on the Equip button is Cross, a right
//! click Triangle. Help: hover walks the list, a click opens the row, a right click is Triangle.
use crate::modern_grid_page::with_grid;
use crate::modern_help as mh;
use crate::modern_help_page as hp;
use crate::modern_qs as mq;
use crate::modern_qs_page as qp;
use crate::modern_tabs::Click;
use rac_engine::ui::font::FontMeasure;
use rac_engine::ui::kit::page::{PageLayout, PageScreen};
use rac_engine::ui::kit::{page_draw, Scale};
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::Gpu;
use std::sync::Mutex;
use winit::event::{ElementState, MouseButton, WindowEvent};

enum Page {
	Qs(qp::Built, (usize, usize)),
	Help(hp::Built, usize),
}

struct Shown {
	page: Page,
	win: (u32, u32),
	hover_cell: Option<(usize, usize)>,
	hover_row: Option<usize>,
	/// [PC] nav lane: the shell (tab bar, hints) as drawn, and the page's tab (tab clicks, modern_tabs.rs).
	shell: (PageLayout, PageScreen, usize),
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);
/// The ring slot the last arrow click asked and when (ms), B18.
static PENDING_RING: Mutex<Option<(u32, u64)>> = Mutex::new(None);

fn ring_now_ms() -> u64 {
	static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
	T0.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

fn lock() -> std::sync::MutexGuard<'static, Option<Shown>> {
	SHOWN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Not drawn this frame (F1, load frames, a page above it): forget the window state, so `event` swallows no clicks.
pub(super) fn clear() {
	*lock() = None;
}

/// Tells the game threads the window can draw these pages.
pub(super) fn set_ready(ok: bool) {
	mq::set_ready(ok);
	mh::set_ready(ok);
}

/// Draws the page when the game thread read Quick Select or Help this field; false (and nothing drawn) otherwise.
pub(super) fn draw(g: &mut Gpu, aspect: f32) -> bool {
	let look = super::screens::look();
	// [PC] nav lane: the cell / row a mouse request is taking the cursor to is drawn selected at once; the shell is kept (tab clicks).
	let (page, prims, hole, shell) = if let Some(mut f) = mq::facts() {
		if let Some(p) = mq::pending().filter(|p| f.cell(p.0, p.1).is_some()) {
			f.sel = p;
		}
		let built = qp::build_look(&f, &FontMeasure, &look); // [b46 W7] B6
		let screen = qp::shell(&f, look);
		let (layout, shell) = page_draw::frame(&screen, &FontMeasure);
		let mut prims = shell;
		prims.extend(built.prims.iter().cloned());
		(Page::Qs(built, f.sel), prims, None, (layout, screen, mq::TAB))
	} else if let Some(mut f) = mh::facts() {
		if let Some(p) = mh::pending().filter(|&p| p < f.rows.len()) {
			f.cur = p;
		}
		let built = hp::build(&f, &FontMeasure, aspect);
		let screen = hp::shell(&f, look);
		let (layout, shell) = page_draw::frame(&screen, &FontMeasure);
		let b = crate::modern_grid_page::Built { prims: built.prims.clone(), hole: built.hole, uv: built.uv, ..Default::default() };
		let (hole, uv) = (built.hole, built.uv);
		(Page::Help(built, f.cur), with_grid(shell, &b), Some((hole, uv)), (layout, screen, mh::TAB))
	} else {
		*lock() = None;
		return super::screens_switch::draw(g); // [PC] nav lane: no sub page up (retail between pages): the kept one while a tab switch runs
	};
	super::screens_switch::keep(&prims, hole.map(|h| h.0));
	if let Some((hole, uv)) = hole {
		// Help: the crop of the row's icon into the hole (filled while the picture is behind: screens_preview.rs).
		super::screens_preview::draw(g, prims, hole, uv);
	} else {
		g.ui_gpu.set_scene(prims); // valid for the next draw only: every frame while the page is up
		g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	}
	let win = g.win_size();
	let mut s = lock();
	let (hover_cell, hover_row) = s.as_ref().map_or((None, None), |s| (s.hover_cell, s.hover_row));
	*s = Some(Shown { page, win, hover_cell, hover_row, shell });
	true
}

/// The window event while a page is up (`ui_pos` = the cursor). True = the page took it.
pub(super) fn event(ui_pos: Option<(f32, f32)>, event: &WindowEvent) -> bool {
	let mut g = lock();
	let Some(s) = g.as_mut() else { return false };
	let design = |x: f32, y: f32| Scale::fit(s.win.0, s.win.1).to_design(x, y);
	match event {
		WindowEvent::CursorMoved { position, .. } => {
			let (dx, dy) = design(position.x as f32, position.y as f32);
			match &s.page {
				Page::Qs(b, sel) => match b.cell_at(dx, dy) {
					Some(c) if s.hover_cell != Some(c) && c != *sel => {
						s.hover_cell = Some(c);
						mq::request(mq::Req { grid: Some(c), ..Default::default() });
					}
					Some(_) => {}
					None => s.hover_cell = None,
				},
				Page::Help(b, cur) => match b.row_at(dx, dy) {
					Some(r) if s.hover_row != Some(r) && r != *cur => {
						s.hover_row = Some(r);
						mh::request(r, false, false);
					}
					Some(_) => {}
					None => s.hover_row = None,
				},
			}
			false
		}
		WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
			let Some((x, y)) = ui_pos else { return true };
			let (dx, dy) = design(x, y);
			// [PC] nav lane: the tab bar and the Back hint (modern_tabs.rs) come first.
			let shell = match button {
				MouseButton::Left => crate::modern_tabs::click(&s.shell.0, &s.shell.1, s.shell.2, dx, dy),
				_ => Click::None,
			};
			match (shell, &s.page) {
				(Click::Tab, _) => return true,
				(Click::Back, Page::Qs(..)) => {
					mq::request(mq::Req { back: true, ..Default::default() });
					return true;
				}
				(Click::Back, Page::Help(..)) => {
					mh::request(0, false, true);
					return true;
				}
				(Click::None, _) => {}
			}
			match (&s.page, button) {
				(Page::Qs(b, sel), MouseButton::Left) => {
					if let Some(c) = b.cell_at(dx, dy) {
						mq::request(mq::Req { grid: Some(c), pick: true, ..Default::default() });
					} else if let Some(slot) = b.slot_at(dx, dy) {
						mq::request(mq::Req { ring: Some(slot), ..Default::default() });
					} else if b.prev.contains(dx, dy) || b.next.contains(dx, dy) {
						let fwd = b.next.contains(dx, dy);
						// [b45 P4A] B18: step from the slot the previous click asked while its walk still runs.
						let cur = mq::facts().map_or(1, |f| f.ring_target());
						let now = ring_now_ms();
						let mut pend = PENDING_RING.lock().unwrap_or_else(|e| e.into_inner());
						let slot = mq::arrow_target(*pend, cur, fwd, now);
						*pend = Some((slot, now));
						mq::request(mq::Req { ring: Some(slot), ..Default::default() });
					} else if b.on_button(dx, dy) {
						mq::request(mq::Req { grid: Some(*sel), pick: true, ..Default::default() });
					}
				}
				(Page::Help(b, _), MouseButton::Left) => {
					if let Some(r) = b.row_at(dx, dy) {
						mh::request(r, true, false);
					}
				}
				(Page::Qs(..), MouseButton::Right) => mq::request(mq::Req { back: true, ..Default::default() }),
				(Page::Help(..), MouseButton::Right) => mh::request(0, false, true),
				_ => {}
			}
			true // a click on the page never reaches the game (the left button is the game's Circle)
		}
		_ => false,
	}
}
