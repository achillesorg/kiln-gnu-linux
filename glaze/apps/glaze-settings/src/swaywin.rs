//! Window-management helpers for Sway, which ignores `xdg_toplevel`
//! `set_minimized` and `set_maximized` for floating windows: minimise is
//! "move to scratchpad" (the Glaze taskbar restores it with `scratchpad show`),
//! maximise is "fill the workspace's usable area" with the old geometry
//! remembered. The JSON parsing is portable and unit-tested.

use serde_json::Value;

/// A rectangle in Sway layout coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

fn rect_of(v: &Value) -> Option<Rect> {
    Some(Rect {
        x: v.get("x")?.as_i64()?,
        y: v.get("y")?.as_i64()?,
        w: v.get("width")?.as_i64()?,
        h: v.get("height")?.as_i64()?,
    })
}

/// Our window in a `get_tree` reply: (window rect, workspace usable rect).
/// Matches `app_id` and, when given, the process id.
pub fn find_window(tree: &Value, app_id: &str, pid: Option<u32>) -> Option<(Rect, Rect)> {
    fn walk(n: &Value, ws: Option<&Value>, app_id: &str, pid: Option<u32>) -> Option<(Rect, Rect)> {
        let ws = if n.get("type").and_then(Value::as_str) == Some("workspace") {
            Some(n)
        } else {
            ws
        };
        if n.get("app_id").and_then(Value::as_str) == Some(app_id)
            && pid.is_none_or(|p| n.get("pid").and_then(Value::as_u64) == Some(p as u64))
        {
            let r = rect_of(n.get("rect")?)?;
            let w = ws.and_then(|w| rect_of(w.get("rect")?))?;
            return Some((r, w));
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(children) = n.get(key).and_then(Value::as_array) {
                for c in children {
                    if let Some(found) = walk(c, ws, app_id, pid) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
    walk(tree, None, app_id, pid)
}

/// True when our window is hidden in the scratchpad (minimised on Sway).
pub fn in_scratchpad(tree: &Value, app_id: &str, pid: Option<u32>) -> bool {
    fn walk(n: &Value, scratch: bool, app_id: &str, pid: Option<u32>) -> Option<bool> {
        let scratch = scratch || n.get("name").and_then(Value::as_str) == Some("__i3_scratch");
        if n.get("app_id").and_then(Value::as_str) == Some(app_id)
            && pid.is_none_or(|p| n.get("pid").and_then(Value::as_u64) == Some(p as u64))
        {
            return Some(scratch);
        }
        ["nodes", "floating_nodes"]
            .iter()
            .filter_map(|k| n.get(*k).and_then(Value::as_array))
            .flatten()
            .find_map(|c| walk(c, scratch, app_id, pid))
    }
    walk(tree, false, app_id, pid).unwrap_or(false)
}

/// True when `win` already covers the usable area (within a few pixels).
pub fn is_filling(win: Rect, area: Rect) -> bool {
    (win.x - area.x).abs() <= 4
        && (win.y - area.y).abs() <= 4
        && (win.w - area.w).abs() <= 8
        && (win.h - area.h).abs() <= 8
}

/// The Sway command that places a floating window at `r`. Resize comes
/// first: Sway resizes a floating window around its centre.
pub fn place_command(criteria: &str, r: Rect) -> String {
    format!(
        "{criteria} resize set width {} px height {} px, move absolute position {} {}",
        r.w, r.h, r.x, r.y
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = r#"{"type":"root","nodes":[{"type":"output","nodes":[
        {"type":"workspace","name":"1","rect":{"x":0,"y":0,"width":1280,"height":760},
         "nodes":[],"floating_nodes":[
            {"type":"floating_con","app_id":"foot","pid":5,"rect":{"x":1,"y":2,"width":3,"height":4}},
            {"type":"floating_con","app_id":"org.glaze.Settings","pid":42,
             "rect":{"x":140,"y":60,"width":1000,"height":640}}]}]}]}"#;

    #[test]
    fn finds_window_and_workspace() {
        let v: Value = serde_json::from_str(TREE).unwrap();
        let (w, a) = find_window(&v, "org.glaze.Settings", Some(42)).unwrap();
        assert_eq!(
            w,
            Rect {
                x: 140,
                y: 60,
                w: 1000,
                h: 640
            }
        );
        assert_eq!(
            a,
            Rect {
                x: 0,
                y: 0,
                w: 1280,
                h: 760
            }
        );
        assert!(find_window(&v, "org.glaze.Settings", Some(7)).is_none());
        assert!(find_window(&v, "nope", None).is_none());
        assert!(!in_scratchpad(&v, "org.glaze.Settings", Some(42)));
        let hidden: Value = serde_json::from_str(
            r#"{"nodes":[{"name":"__i3_scratch","type":"workspace","floating_nodes":[
                {"app_id":"org.glaze.Settings","pid":42,"rect":{"x":0,"y":0,"width":1,"height":1}}]}]}"#,
        )
        .unwrap();
        assert!(in_scratchpad(&hidden, "org.glaze.Settings", Some(42)));
    }

    #[test]
    fn filling_and_commands() {
        let a = Rect {
            x: 0,
            y: 0,
            w: 1280,
            h: 760,
        };
        assert!(is_filling(
            Rect {
                x: 0,
                y: 2,
                w: 1280,
                h: 756
            },
            a
        ));
        assert!(!is_filling(
            Rect {
                x: 140,
                y: 60,
                w: 1000,
                h: 640
            },
            a
        ));
        assert_eq!(
            place_command(
                "[app_id=\"x\"]",
                Rect {
                    x: 1,
                    y: 2,
                    w: 3,
                    h: 4
                }
            ),
            "[app_id=\"x\"] resize set width 3 px height 4 px, move absolute position 1 2"
        );
    }
}
