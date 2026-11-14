//! `glaze-vptr`: a virtual pointer for headless Sway test sessions.
//!
//! Creates a `zwlr_virtual_pointer_v1` (so the seat gains pointer capability)
//! and executes one command per stdin line, in output-layout pixels:
//!
//! ```text
//! move X Y        absolute motion
//! click X Y       move, left press + release
//! dclick X Y      double click
//! rclick X Y      move, right press + release
//! scroll DY       vertical wheel (positive = down), in steps of 15
//! sleep MS
//! ```
//! Usage: `glaze-vptr WIDTH HEIGHT < commands` (layout extent, e.g. 1280 800).

use std::io::BufRead;
use std::time::{Duration, Instant};

use wayland_client::protocol::{wl_pointer, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

#[derive(Default)]
struct State {
    seat: Option<wl_seat::WlSeat>,
    manager: Option<ZwlrVirtualPointerManagerV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        st: &mut Self,
        reg: &wl_registry::WlRegistry,
        ev: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = ev
        {
            match interface.as_str() {
                "wl_seat" if st.seat.is_none() => {
                    st.seat = Some(reg.bind(name, version.min(7), qh, ()));
                }
                "zwlr_virtual_pointer_manager_v1" => {
                    st.manager = Some(reg.bind(name, version.min(2), qh, ()));
                }
                _ => {}
            }
        }
    }
}

macro_rules! ignore_events {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for State {
            fn event(_: &mut Self, _: &$t, _: <$t as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}
ignore_events!(
    wl_seat::WlSeat,
    ZwlrVirtualPointerManagerV1,
    ZwlrVirtualPointerV1
);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let w: u32 = args.first().and_then(|s| s.parse().ok()).unwrap_or(1280);
    let h: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(800);
    let conn = Connection::connect_to_env().expect("glaze-vptr: no Wayland display");
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut st = State::default();
    queue.roundtrip(&mut st).expect("roundtrip");
    let manager = st
        .manager
        .clone()
        .expect("glaze-vptr: compositor lacks zwlr_virtual_pointer_manager_v1");
    let ptr = manager.create_virtual_pointer(st.seat.as_ref(), &qh, ());
    queue.roundtrip(&mut st).expect("roundtrip");
    let start = Instant::now();
    let t = || start.elapsed().as_millis() as u32;
    let flush = |q: &mut wayland_client::EventQueue<State>, st: &mut State| {
        q.roundtrip(st).expect("roundtrip");
    };
    let motion = |x: u32, y: u32| {
        ptr.motion_absolute(t(), x.min(w - 1), y.min(h - 1), w, h);
        ptr.frame();
    };
    let button = |b: u32| {
        ptr.button(t(), b, wl_pointer::ButtonState::Pressed);
        ptr.frame();
        conn.flush().ok();
        std::thread::sleep(Duration::from_millis(60));
        ptr.button(t(), b, wl_pointer::ButtonState::Released);
        ptr.frame();
    };
    println!("ready");
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let f: Vec<&str> = line.split_whitespace().collect();
        let n = |i: usize| f.get(i).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        match f.first().copied() {
            Some("move") => motion(n(1) as u32, n(2) as u32),
            Some("click") | Some("rclick") => {
                motion(n(1) as u32, n(2) as u32);
                flush(&mut queue, &mut st);
                std::thread::sleep(Duration::from_millis(40));
                button(if f[0] == "click" { BTN_LEFT } else { BTN_RIGHT });
            }
            Some("dclick") => {
                motion(n(1) as u32, n(2) as u32);
                flush(&mut queue, &mut st);
                std::thread::sleep(Duration::from_millis(40));
                button(BTN_LEFT);
                flush(&mut queue, &mut st);
                std::thread::sleep(Duration::from_millis(80));
                button(BTN_LEFT);
            }
            Some("scroll") => {
                ptr.axis(t(), wl_pointer::Axis::VerticalScroll, n(1) as f64 * 15.0);
                ptr.frame();
            }
            Some("sleep") => std::thread::sleep(Duration::from_millis(n(1) as u64)),
            Some(other) => eprintln!("glaze-vptr: unknown command {other}"),
            None => {}
        }
        flush(&mut queue, &mut st);
        println!("ok {line}");
    }
    ptr.destroy();
    flush(&mut queue, &mut st);
}
