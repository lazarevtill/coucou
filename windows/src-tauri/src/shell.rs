// The Windows shell around Coucou: the tray icon's state, the flyout's place next
// to it, and toast notifications. Only the pure parts live here, so they are
// tested directly; tray.rs, flyout.rs and platform/windows/toast.rs do the OS work.

use std::time::Duration;

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// The screen edge the taskbar sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

pub fn taskbar_edge(icon: PRect, work: PRect) -> Edge {
    let cx = icon.x as i64 + icon.w as i64 / 2;
    let cy = icon.y as i64 + icon.h as i64 / 2;
    let (left, top) = (work.x as i64, work.y as i64);
    let (right, bottom) = (left + work.w as i64, top + work.h as i64);
    if cy >= bottom {
        return Edge::Bottom;
    }
    if cy < top {
        return Edge::Top;
    }
    if cx < left {
        return Edge::Left;
    }
    if cx >= right {
        return Edge::Right;
    }
    // Inside the work area: the overflow panel, or a taskbar that hides itself.
    let distances = [(bottom - cy, Edge::Bottom), (cy - top, Edge::Top), (cx - left, Edge::Left), (right - cx, Edge::Right)];
    distances.iter().min_by_key(|(d, _)| *d).map(|(_, e)| *e).unwrap_or(Edge::Bottom)
}

/// Where the flyout's top-left corner goes: against the taskbar, next to the icon,
/// `margin` pixels from the edges, and always inside the work area.
pub fn flyout_origin(icon: PRect, work: PRect, size: (u32, u32), margin: i32) -> (i32, i32) {
    let (w, h) = (size.0 as i64, size.1 as i64);
    let m = margin as i64;
    let cx = icon.x as i64 + icon.w as i64 / 2;
    let cy = icon.y as i64 + icon.h as i64 / 2;
    let (left, top) = (work.x as i64, work.y as i64);
    let (right, bottom) = (left + work.w as i64, top + work.h as i64);
    let (x, y) = match taskbar_edge(icon, work) {
        Edge::Bottom => (cx - w / 2, bottom - h - m),
        Edge::Top => (cx - w / 2, top + m),
        Edge::Left => (left + m, cy - h / 2),
        Edge::Right => (right - w - m, cy - h / 2),
    };
    // Bigger than the work area: pin to its top-left rather than overflow both ways.
    let x = x.min(right - w - m).max(left + m);
    let y = y.min(bottom - h - m).max(top + m);
    (x as i32, y as i32)
}

/// Windows shows at most 127 UTF-16 units of a tray tooltip.
const TOOLTIP_UNITS: usize = 127;

pub fn tooltip(text: &str) -> String {
    if text.encode_utf16().count() <= TOOLTIP_UNITS {
        return text.to_string();
    }
    let mut out = String::new();
    let mut units = 0;
    for c in text.chars() {
        // One unit is kept for the ellipsis.
        if units + c.len_utf16() > TOOLTIP_UNITS - 1 {
            break;
        }
        units += c.len_utf16();
        out.push(c);
    }
    out.push('…');
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    Working,
    Attention,
    Error,
    Done,
    Paused,
}

impl TrayState {
    pub fn parse(s: &str) -> Self {
        match s {
            "working" => TrayState::Working,
            "attention" => TrayState::Attention,
            "error" => TrayState::Error,
            "done" => TrayState::Done,
            "paused" => TrayState::Paused,
            _ => TrayState::Idle,
        }
    }

    /// The dot drawn on the tray icon; the same colours as the island's states.
    pub fn badge(self) -> Option<[u8; 3]> {
        match self {
            TrayState::Idle => None,
            TrayState::Working => Some([59, 158, 255]),   // #3B9EFF
            TrayState::Attention => Some([245, 165, 36]), // #F5A524
            TrayState::Error => Some([244, 80, 94]),      // #F4505E
            TrayState::Done => Some([52, 211, 153]),      // #34D399
            TrayState::Paused => Some([142, 147, 156]),   // #8E939C
        }
    }
}

/// `rgba` with a round badge in the bottom-right corner, ringed in near-black so it
/// reads on light and dark taskbars alike.
pub fn badged(rgba: &[u8], w: u32, h: u32, color: [u8; 3]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    if w == 0 || h == 0 || rgba.len() != (w as usize) * (h as usize) * 4 {
        return out;
    }
    let size = w.min(h) as f32;
    let (cx, cy) = (w as f32 * 0.78, h as f32 * 0.78);
    let radius = size * 0.2;
    let ring = (size * 0.045).max(1.0);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let px = if d <= radius {
                [color[0], color[1], color[2], 255]
            } else if d <= radius + ring {
                [20, 20, 22, 255]
            } else {
                continue;
            };
            let i = ((y * w + x) * 4) as usize;
            out[i..i + 4].copy_from_slice(&px);
        }
    }
    out
}

/// A click on the tray icon that arrives this soon after the flyout hid itself is
/// the same click that took its focus away: it closed the flyout, it must not
/// reopen it.
const REOPEN_GRACE: Duration = Duration::from_millis(300);

pub fn should_open(visible: bool, hidden_ago: Option<Duration>) -> bool {
    !visible && hidden_ago.map_or(true, |ago| ago > REOPEN_GRACE)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToastSpec {
    pub tag: String,
    pub title: String,
    pub body: String,
    /// The session the button jumps to; no button without one.
    pub jump: Option<String>,
    pub jump_label: String,
}

const MAX_TITLE: usize = 80;
const MAX_BODY: usize = 240;
const MAX_ID: usize = 128;

fn escape(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, c) in text.chars().enumerate() {
        if i == max_chars {
            out.push('…');
            break;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\n' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn plain_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID && !id.chars().any(|c| c.is_control() || c == '"' || c == '<' || c == '&')
}

/// The toast document. Clicking the toast opens Coucou; the one button goes to
/// the session. There is never an Allow button: a toast cuts text, and what is
/// approved has to be seen whole.
pub fn toast_xml(spec: &ToastSpec) -> String {
    let mut actions = String::new();
    if let Some(id) = spec.jump.as_deref().filter(|id| plain_id(id)) {
        actions.push_str(&format!(
            r#"<action content="{}" arguments="jump:{}"/>"#,
            escape(&spec.jump_label, 40),
            escape(id, MAX_ID)
        ));
    }
    actions.push_str(r#"<action content="Dismiss" arguments="dismiss" activationType="system"/>"#);
    format!(
        r#"<toast launch="open"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual><actions>{}</actions></toast>"#,
        escape(&spec.title, MAX_TITLE),
        escape(&spec.body, MAX_BODY),
        actions
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activation {
    Jump(String),
    Open,
}

/// What a toast click sends back. Anything that is not a well-formed jump opens
/// Coucou, which is what clicking the toast itself does.
pub fn parse_activation(args: &str) -> Activation {
    match args.strip_prefix("jump:") {
        Some(id) if plain_id(id) => Activation::Jump(id.to_string()),
        _ => Activation::Open,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A 1920x1080 display at 100 %, taskbar 48 px.
    const SCREEN_W: u32 = 1920;
    const SCREEN_H: u32 = 1080;
    const FLY: (u32, u32) = (380, 520);

    fn r(x: i32, y: i32, w: u32, h: u32) -> PRect {
        PRect { x, y, w, h }
    }

    #[test]
    fn the_taskbar_edge_is_where_the_icon_sits_outside_the_work_area() {
        assert_eq!(taskbar_edge(r(1700, 1040, 24, 24), r(0, 0, SCREEN_W, 1032)), Edge::Bottom);
        assert_eq!(taskbar_edge(r(1700, 12, 24, 24), r(0, 48, SCREEN_W, 1032)), Edge::Top);
        assert_eq!(taskbar_edge(r(12, 900, 24, 24), r(62, 0, 1858, SCREEN_H)), Edge::Left);
        assert_eq!(taskbar_edge(r(1880, 900, 24, 24), r(0, 0, 1858, SCREEN_H)), Edge::Right);
    }

    #[test]
    fn an_icon_inside_the_work_area_goes_by_the_nearest_edge() {
        // The overflow ("show hidden icons") panel, or an auto-hidden taskbar.
        assert_eq!(taskbar_edge(r(1800, 980, 24, 24), r(0, 0, SCREEN_W, SCREEN_H)), Edge::Bottom);
        assert_eq!(taskbar_edge(r(1890, 400, 24, 24), r(0, 0, SCREEN_W, SCREEN_H)), Edge::Right);
    }

    #[test]
    fn with_the_taskbar_at_the_bottom_the_flyout_sits_above_the_icon() {
        let (x, y) = flyout_origin(r(1500, 1040, 24, 24), r(0, 0, SCREEN_W, 1032), FLY, 12);
        assert_eq!(y, 1032 - 520 - 12, "just above the taskbar");
        assert_eq!(x, 1512 - 190, "centred on the icon");
    }

    #[test]
    fn the_flyout_never_leaves_the_work_area() {
        // Icon at the far right: the flyout is pushed back in.
        let (x, _) = flyout_origin(r(1900, 1040, 24, 24), r(0, 0, SCREEN_W, 1032), FLY, 12);
        assert_eq!(x, 1920 - 380 - 12);
        // Taskbar on the left, icon near the bottom.
        let (x, y) = flyout_origin(r(12, 1050, 24, 24), r(62, 0, 1858, SCREEN_H), FLY, 12);
        assert_eq!(x, 62 + 12);
        assert_eq!(y, 1080 - 520 - 12);
        // Taskbar at the top.
        let (_, y) = flyout_origin(r(1700, 12, 24, 24), r(0, 48, SCREEN_W, 1032), FLY, 12);
        assert_eq!(y, 48 + 12);
        // Taskbar on the right.
        let (x, _) = flyout_origin(r(1880, 500, 24, 24), r(0, 0, 1858, SCREEN_H), FLY, 12);
        assert_eq!(x, 1858 - 380 - 12);
    }

    #[test]
    fn a_second_display_left_of_the_first_has_negative_coordinates() {
        let work = r(-2560, 0, 2560, 1392);
        let (x, y) = flyout_origin(r(-200, 1400, 32, 32), work, (570, 780), 18);
        assert_eq!(y, 1392 - 780 - 18);
        assert_eq!(x, -570 - 18, "pushed back inside the left display");
    }

    #[test]
    fn the_tooltip_fits_windows_limit_and_cuts_on_a_character() {
        assert_eq!(tooltip("Coucou — idle"), "Coucou — idle");
        let long = "é".repeat(200);
        let cut = tooltip(&long);
        assert!(cut.encode_utf16().count() <= 127, "{}", cut.encode_utf16().count());
        assert!(cut.ends_with('…'));
        // Characters outside the BMP are two UTF-16 units; never split one.
        let emoji = "🙂".repeat(100);
        let cut = tooltip(&emoji);
        assert!(cut.encode_utf16().count() <= 127);
        assert!(cut.chars().all(|c| c == '🙂' || c == '…'));
    }

    #[test]
    fn tray_states_parse_and_only_some_wear_a_badge() {
        assert_eq!(TrayState::parse("attention"), TrayState::Attention);
        assert_eq!(TrayState::parse("working"), TrayState::Working);
        assert_eq!(TrayState::parse("error"), TrayState::Error);
        assert_eq!(TrayState::parse("done"), TrayState::Done);
        assert_eq!(TrayState::parse("paused"), TrayState::Paused);
        assert_eq!(TrayState::parse("idle"), TrayState::Idle);
        assert_eq!(TrayState::parse("nonsense"), TrayState::Idle);
        assert_eq!(TrayState::Idle.badge(), None);
        let colours: Vec<_> = [TrayState::Attention, TrayState::Working, TrayState::Error, TrayState::Done, TrayState::Paused]
            .iter()
            .map(|s| s.badge().expect("a badge"))
            .collect();
        for (i, a) in colours.iter().enumerate() {
            for b in &colours[i + 1..] {
                assert_ne!(a, b, "every state has its own colour");
            }
        }
    }

    #[test]
    fn a_badge_is_drawn_in_the_bottom_right_corner_and_nowhere_else() {
        let (w, h) = (32u32, 32u32);
        let base = vec![10u8; (w * h * 4) as usize];
        let out = badged(&base, w, h, [245, 165, 36]);
        assert_eq!(out.len(), base.len());
        let px = |x: u32, y: u32| &out[((y * w + x) * 4) as usize..((y * w + x) * 4 + 4) as usize];
        assert_eq!(px(25, 25), &[245, 165, 36, 255], "badge centre");
        assert_eq!(px(4, 4), &[10, 10, 10, 10], "top-left untouched");
        assert_eq!(px(16, 16), &[10, 10, 10, 10], "centre untouched");
        // A wrong-sized buffer is returned as it is rather than drawn out of bounds.
        assert_eq!(badged(&[1, 2, 3], w, h, [0, 0, 0]), vec![1, 2, 3]);
    }

    #[test]
    fn the_click_that_closed_the_flyout_does_not_reopen_it() {
        // Clicking the tray icon while the flyout is open: the flyout loses focus
        // and hides first, then the click arrives.
        assert!(!should_open(false, Some(Duration::from_millis(80))));
        assert!(should_open(false, Some(Duration::from_millis(900))));
        assert!(should_open(false, None));
        assert!(!should_open(true, None), "a click on an open flyout closes it");
    }

    fn spec(jump: Option<&str>) -> ToastSpec {
        ToastSpec {
            tag: "s1".into(),
            title: "coucou asks in Windows Terminal".into(),
            body: "Use <b> & \"quotes\"?".into(),
            jump: jump.map(str::to_string),
            jump_label: "Go to Windows Terminal".into(),
        }
    }

    #[test]
    fn toast_text_is_escaped_and_the_button_carries_the_session() {
        let xml = toast_xml(&spec(Some("abc-123")));
        assert!(xml.starts_with("<toast"));
        assert!(xml.contains("Use &lt;b&gt; &amp; &quot;quotes&quot;?"), "{xml}");
        assert!(xml.contains(r#"arguments="jump:abc-123""#), "{xml}");
        assert!(xml.contains(r#"content="Go to Windows Terminal""#), "{xml}");
        assert!(xml.contains(r#"launch="open""#), "clicking the toast itself opens Coucou");
        assert!(!xml.to_lowercase().contains("allow"), "never an Allow button on a toast");
    }

    #[test]
    fn a_toast_without_a_session_has_no_jump_button() {
        let xml = toast_xml(&spec(None));
        assert!(!xml.contains("jump:"), "{xml}");
    }

    #[test]
    fn long_toast_text_is_cut() {
        let mut s = spec(Some("x"));
        s.body = "a".repeat(5000);
        let xml = toast_xml(&s);
        assert!(xml.len() < 2000, "{}", xml.len());
    }

    #[test]
    fn what_a_toast_click_sends_back_is_read_strictly() {
        assert_eq!(parse_activation("jump:abc-123"), Activation::Jump("abc-123".into()));
        assert_eq!(parse_activation("open"), Activation::Open);
        assert_eq!(parse_activation(""), Activation::Open);
        assert_eq!(parse_activation("jump:"), Activation::Open);
        assert_eq!(parse_activation(&format!("jump:{}", "x".repeat(200))), Activation::Open, "too long for a session id");
        assert_eq!(parse_activation("jump:a\nb"), Activation::Open, "control characters");
    }
}
