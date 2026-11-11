//! macOS: the window content runs under a transparent title bar (main.rs),
//! and the app draws its own 44 px title bar (app.rs `top_bar`). AppKit keeps
//! the traffic lights where a 28 px title bar would have them; this moves
//! them to the middle of the app's bar, 14 px from the left, 8 px apart (the
//! prototype's `.lights`). AppKit puts them back on resize and full screen,
//! so it runs every frame (a few cheap calls).

use objc2_app_kit::{NSView, NSWindowButton};
use objc2_foundation::NSPoint;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub fn center_traffic_lights(frame: &eframe::Frame, bar_height: f64) {
    let Ok(handle) = frame.window_handle() else { return };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return };
    // SAFETY: the handle is the window's live NSView for the duration of the frame.
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    let Some(window) = view.window() else { return };
    let buttons = [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton];
    let Some(close) = window.standardWindowButton(buttons[0]) else { return };
    // The buttons sit in the title bar view, inside its container: make the
    // container as tall as the app's bar so they can be centered in it.
    // SAFETY: plain view hierarchy reads on the main thread.
    let Some(container) = (unsafe { close.superview().and_then(|v| v.superview()) }) else { return };
    let window_h = window.frame().size.height;
    let mut rect = container.frame();
    if (rect.size.height - bar_height).abs() > 0.5 || (rect.origin.y - (window_h - bar_height)).abs() > 0.5 {
        rect.size.height = bar_height;
        rect.origin.y = window_h - bar_height;
        container.setFrame(rect);
    }
    let mut x = 14.0;
    for kind in buttons {
        let Some(b) = window.standardWindowButton(kind) else { continue };
        let f = b.frame();
        let origin = NSPoint::new(x, ((bar_height - f.size.height) / 2.0).round());
        if (f.origin.x - origin.x).abs() > 0.5 || (f.origin.y - origin.y).abs() > 0.5 {
            b.setFrameOrigin(origin);
        }
        x += f.size.width + 8.0;
    }
}
