//! `glaze-toplevel-ctl`: a tiny wlr-foreign-toplevel-management client, used
//! to check (in scripts) that a compositor lets a taskbar list and drive its
//! windows. Works against glaze-compositor, Sway, labwc, ...
//!
//! Usage: glaze-toplevel-ctl list
//!        glaze-toplevel-ctl <activate|minimize|unminimize|maximize|unmaximize|close> <INDEX|TITLE-SUBSTRING>

#[cfg(target_os = "linux")]
mod imp {
    use wayland_client::protocol::{wl_registry, wl_seat};
    use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
    use wayland_protocols_wlr::foreign_toplevel::v1::client::{
        zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
        zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
    };

    #[derive(Default, Clone)]
    struct Info {
        title: String,
        app_id: String,
        states: Vec<u32>,
        closed: bool,
    }

    #[derive(Default)]
    struct App {
        manager: Option<ZwlrForeignToplevelManagerV1>,
        seat: Option<wl_seat::WlSeat>,
        handles: Vec<(ZwlrForeignToplevelHandleV1, Info)>,
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for App {
        fn event(
            app: &mut Self,
            registry: &wl_registry::WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            {
                if interface == ZwlrForeignToplevelManagerV1::interface().name {
                    app.manager = Some(registry.bind(name, version.min(3), qh, ()));
                } else if interface == wl_seat::WlSeat::interface().name && app.seat.is_none() {
                    app.seat = Some(registry.bind(name, version.min(7), qh, ()));
                }
            }
        }
    }

    impl Dispatch<wl_seat::WlSeat, ()> for App {
        fn event(
            _: &mut Self,
            _: &wl_seat::WlSeat,
            _: wl_seat::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for App {
        fn event(
            app: &mut Self,
            _: &ZwlrForeignToplevelManagerV1,
            event: zwlr_foreign_toplevel_manager_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
                app.handles.push((toplevel, Info::default()));
            }
        }

        event_created_child!(App, ZwlrForeignToplevelManagerV1, [
            zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
        ]);
    }

    impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for App {
        fn event(
            app: &mut Self,
            handle: &ZwlrForeignToplevelHandleV1,
            event: zwlr_foreign_toplevel_handle_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            use zwlr_foreign_toplevel_handle_v1::Event;
            let Some((_, info)) = app.handles.iter_mut().find(|(h, _)| h == handle) else {
                return;
            };
            match event {
                Event::Title { title } => info.title = title,
                Event::AppId { app_id } => info.app_id = app_id,
                Event::State { state } => {
                    info.states = state
                        .chunks_exact(4)
                        .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
                        .collect()
                }
                Event::Closed => info.closed = true,
                _ => {}
            }
        }
    }

    fn state_names(states: &[u32]) -> String {
        states
            .iter()
            .map(|s| match s {
                0 => "maximized",
                1 => "minimized",
                2 => "activated",
                3 => "fullscreen",
                _ => "?",
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn main() -> Result<(), String> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let conn = Connection::connect_to_env().map_err(|e| format!("connect: {e}"))?;
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        let _registry = conn.display().get_registry(&qh, ());
        let mut app = App::default();
        for _ in 0..3 {
            queue.roundtrip(&mut app).map_err(|e| e.to_string())?;
        }
        if app.manager.is_none() {
            return Err("compositor has no zwlr_foreign_toplevel_manager_v1".into());
        }
        let live: Vec<(ZwlrForeignToplevelHandleV1, Info)> = app
            .handles
            .iter()
            .filter(|(_, i)| !i.closed)
            .cloned()
            .collect();
        match args.first().map(String::as_str) {
            None | Some("list") => {
                for (i, (_, info)) in live.iter().enumerate() {
                    println!(
                        "{i}\t{}\t{}\t[{}]",
                        info.app_id,
                        info.title,
                        state_names(&info.states)
                    );
                }
                return Ok(());
            }
            Some(cmd) => {
                let target = args
                    .get(1)
                    .ok_or("missing window (index or title substring)")?;
                let found = target
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| live.get(n))
                    .or_else(|| live.iter().find(|(_, i)| i.title.contains(target.as_str())))
                    .ok_or_else(|| format!("no window matches {target}"))?;
                let h = &found.0;
                match cmd {
                    "activate" => h.activate(app.seat.as_ref().ok_or("no seat")?),
                    "minimize" => h.set_minimized(),
                    "unminimize" => h.unset_minimized(),
                    "maximize" => h.set_maximized(),
                    "unmaximize" => h.unset_maximized(),
                    "close" => h.close(),
                    other => return Err(format!("unknown command {other}")),
                }
                queue.roundtrip(&mut app).map_err(|e| e.to_string())?;
                println!("{cmd} sent to {:?}", found.1.title);
            }
        }
        Ok(())
    }
}

fn main() -> std::process::ExitCode {
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = imp::main() {
            eprintln!("glaze-toplevel-ctl: {e}");
            return std::process::ExitCode::FAILURE;
        }
        std::process::ExitCode::SUCCESS
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("glaze-toplevel-ctl: Linux only");
        std::process::ExitCode::FAILURE
    }
}
