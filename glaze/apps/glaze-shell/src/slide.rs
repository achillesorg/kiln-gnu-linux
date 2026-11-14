//! `SlideBin`: a one-child container that draws its child shifted down by
//! `offset` px and clipped to its own box, without any relayout.
//!
//! Used for the Start menu's open/close motion (Windows 10/11 style: the
//! card slides up out of the taskbar edge while fading in). Animating a
//! margin instead resized the popover surface on every frame, which Sway
//! does not re-place cleanly (the visible jump/flicker on open). Here the
//! surface size never changes: each frame only sets a number and queues a
//! redraw; the snapshot translation uses a stack value (no allocation).

use std::cell::Cell;

use gtk4::glib;
use gtk4::graphene;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct SlideBin {
        pub offset: Cell<f32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SlideBin {
        const NAME: &'static str = "GlazeSlideBin";
        type Type = super::SlideBin;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk4::BinLayout>();
        }
    }

    impl ObjectImpl for SlideBin {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for SlideBin {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let obj = self.obj();
            let Some(child) = obj.first_child() else {
                return;
            };
            let off = self.offset.get();
            if off.abs() < 0.01 {
                obj.snapshot_child(&child, snapshot);
                return;
            }
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, w, h));
            snapshot.save();
            snapshot.translate(&graphene::Point::new(0.0, off));
            obj.snapshot_child(&child, snapshot);
            snapshot.restore();
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct SlideBin(ObjectSubclass<imp::SlideBin>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl SlideBin {
    pub fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let bin: Self = glib::Object::new();
        child.set_parent(&bin);
        bin.set_overflow(gtk4::Overflow::Hidden);
        bin
    }

    /// Draw the child `px` lower (clipped); 0 = at rest.
    pub fn set_offset(&self, px: f64) {
        let v = px as f32;
        if (self.imp().offset.get() - v).abs() > 0.01 {
            self.imp().offset.set(v);
            self.queue_draw();
        }
    }
}
