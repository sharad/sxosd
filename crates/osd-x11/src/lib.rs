use osd_core::{Renderer, Scene, Size};
use std::ffi::CString;
use std::mem;
use std::os::raw::{c_char, c_int, c_long};
use std::ptr;
use thiserror::Error;
use x11::xlib;

const SHAPE_BOUNDING: c_int = 0;
const SHAPE_INPUT: c_int = 2;
const SHAPE_SET: c_int = 0;
const UNSORTED: c_int = 0;

#[repr(C)]
#[derive(Clone, Copy)]
struct XineramaScreenInfo {
    screen_number: c_int,
    x_org: i16,
    y_org: i16,
    width: i16,
    height: i16,
}

unsafe extern "C" {
    fn XShapeQueryExtension(
        display: *mut xlib::Display,
        event_base_return: *mut c_int,
        error_base_return: *mut c_int,
    ) -> c_int;

    fn XShapeCombineMask(
        display: *mut xlib::Display,
        dest: xlib::Window,
        dest_kind: c_int,
        x_off: c_int,
        y_off: c_int,
        src: xlib::Pixmap,
        op: c_int,
    );

    fn XineramaQueryExtension(
        display: *mut xlib::Display,
        event_base_return: *mut c_int,
        error_base_return: *mut c_int,
    ) -> c_int;

    fn XineramaIsActive(display: *mut xlib::Display) -> c_int;

    fn XineramaQueryScreens(
        display: *mut xlib::Display,
        number: *mut c_int,
    ) -> *mut XineramaScreenInfo;

    fn XShapeCombineRectangles(
        display: *mut xlib::Display,
        dest: xlib::Window,
        dest_kind: c_int,
        x_off: c_int,
        y_off: c_int,
        rectangles: *mut xlib::XRectangle,
        n_rects: c_int,
        op: c_int,
        ordering: c_int,
    );
}

#[derive(Debug, Error)]
pub enum X11Error {
    #[error("XOpenDisplay failed")]
    OpenDisplay,
    #[error("X Shape extension is unavailable")]
    ShapeUnavailable,
    #[error("XCreateWindow failed")]
    CreateWindow,
    #[error("XCreatePixmap failed")]
    CreatePixmap,
    #[error("XCreateGC failed")]
    CreateGc,
    #[error("XCreateFontSet failed for font {0}")]
    OpenFont(String),
    #[error("XAllocColor failed for color {0}")]
    ColorAlloc(String),
}

/// X11 renderer following the rendering model used by XOSD/osd_cat:
///
/// * unmanaged override-redirect X11 window
/// * screen-depth pixmap containing the actual text
/// * 1-bit pixmap containing the exact text mask
/// * XShape bounding shape derived from that mask
/// * XCopyArea from the text pixmap into the window
///
/// This does not require an X compositor and does not use an ARGB visual.
pub struct X11Renderer {
    display: *mut xlib::Display,
    window: xlib::Window,
    line_bitmap: xlib::Pixmap,
    mask_bitmap: xlib::Pixmap,
    gc: xlib::GC,
    mask_gc: xlib::GC,
    mask_gc_back: xlib::GC,
    fontset: xlib::XFontSet,
    color: xlib::XColor,
    screen: c_int,
    width: u32,
    height: u32,
}

impl X11Renderer {
    pub fn new(font_name: &str, foreground: &str) -> Result<Self, X11Error> {
        unsafe {
            let display = xlib::XOpenDisplay(ptr::null());
            if display.is_null() {
                return Err(X11Error::OpenDisplay);
            }

            let screen = xlib::XDefaultScreen(display);
            let root = xlib::XRootWindow(display, screen);
            let visual = xlib::XDefaultVisual(display, screen);
            let depth = xlib::XDefaultDepth(display, screen);
            let colormap = xlib::XDefaultColormap(display, screen);
            // Match XOSD's default xosd_monitor(1): use the first
            // Xinerama monitor when available, otherwise the X11 screen.
            let mut screen_x = 0;
            let mut screen_y = 0;
            let mut width = xlib::XDisplayWidth(display, screen) as u32;
            let mut height = xlib::XDisplayHeight(display, screen) as u32;
            let mut xinerama_event = 0;
            let mut xinerama_error = 0;
            if XineramaQueryExtension(display, &mut xinerama_event, &mut xinerama_error) != 0
                && XineramaIsActive(display) != 0
            {
                let mut count = 0;
                let monitors = XineramaQueryScreens(display, &mut count);
                if !monitors.is_null() && count > 0 {
                    let monitor = *monitors;
                    screen_x = monitor.x_org as c_int;
                    screen_y = monitor.y_org as c_int;
                    width = monitor.width as u32;
                    height = monitor.height as u32;
                    xlib::XFree(monitors.cast());
                }
            }

            let mut shape_event = 0;
            let mut shape_error = 0;
            if XShapeQueryExtension(display, &mut shape_event, &mut shape_error) == 0 {
                xlib::XCloseDisplay(display);
                return Err(X11Error::ShapeUnavailable);
            }

            let mut attrs: xlib::XSetWindowAttributes = mem::zeroed();
            attrs.override_redirect = 1;
            attrs.event_mask = xlib::ExposureMask as c_long | xlib::StructureNotifyMask as c_long;

            let window = xlib::XCreateWindow(
                display,
                root,
                screen_x,
                screen_y,
                width,
                height,
                0,
                depth,
                xlib::InputOutput as u32,
                visual,
                xlib::CWOverrideRedirect | xlib::CWEventMask,
                &mut attrs,
            );
            if window == 0 {
                xlib::XCloseDisplay(display);
                return Err(X11Error::CreateWindow);
            }

            let line_bitmap = xlib::XCreatePixmap(
                display,
                window,
                width,
                height,
                depth as u32,
            );
            let mask_bitmap = xlib::XCreatePixmap(display, window, width, height, 1);
            if line_bitmap == 0 || mask_bitmap == 0 {
                if line_bitmap != 0 {
                    xlib::XFreePixmap(display, line_bitmap);
                }
                if mask_bitmap != 0 {
                    xlib::XFreePixmap(display, mask_bitmap);
                }
                xlib::XDestroyWindow(display, window);
                xlib::XCloseDisplay(display);
                return Err(X11Error::CreatePixmap);
            }

            let mut gcv: xlib::XGCValues = mem::zeroed();
            let gc = xlib::XCreateGC(display, window, 0, &mut gcv);
            let mask_gc = xlib::XCreateGC(display, mask_bitmap, 0, &mut gcv);
            let mask_gc_back = xlib::XCreateGC(display, mask_bitmap, 0, &mut gcv);
            if gc.is_null() || mask_gc.is_null() || mask_gc_back.is_null() {
                if !gc.is_null() {
                    xlib::XFreeGC(display, gc);
                }
                if !mask_gc.is_null() {
                    xlib::XFreeGC(display, mask_gc);
                }
                if !mask_gc_back.is_null() {
                    xlib::XFreeGC(display, mask_gc_back);
                }
                xlib::XFreePixmap(display, line_bitmap);
                xlib::XFreePixmap(display, mask_bitmap);
                xlib::XDestroyWindow(display, window);
                xlib::XCloseDisplay(display);
                return Err(X11Error::CreateGc);
            }

            let font_c = CString::new(font_name).map_err(|_| X11Error::OpenFont(font_name.into()))?;
            let mut missing: *mut *mut c_char = ptr::null_mut();
            let mut nmissing = 0;
            let mut default_string: *mut c_char = ptr::null_mut();
            let fontset = xlib::XCreateFontSet(
                display,
                font_c.as_ptr(),
                &mut missing,
                &mut nmissing,
                &mut default_string,
            );
            if !missing.is_null() {
                xlib::XFreeStringList(missing);
            }
            if fontset.is_null() {
                xlib::XFreeGC(display, gc);
                xlib::XFreeGC(display, mask_gc);
                xlib::XFreeGC(display, mask_gc_back);
                xlib::XFreePixmap(display, line_bitmap);
                xlib::XFreePixmap(display, mask_bitmap);
                xlib::XDestroyWindow(display, window);
                xlib::XCloseDisplay(display);
                return Err(X11Error::OpenFont(font_name.into()));
            }

            let color_c = CString::new(foreground).map_err(|_| X11Error::ColorAlloc(foreground.into()))?;
            let mut color: xlib::XColor = mem::zeroed();
            if xlib::XParseColor(display, colormap, color_c.as_ptr(), &mut color) == 0
                || xlib::XAllocColor(display, colormap, &mut color) == 0
            {
                xlib::XFreeFontSet(display, fontset);
                xlib::XFreeGC(display, gc);
                xlib::XFreeGC(display, mask_gc);
                xlib::XFreeGC(display, mask_gc_back);
                xlib::XFreePixmap(display, line_bitmap);
                xlib::XFreePixmap(display, mask_bitmap);
                xlib::XDestroyWindow(display, window);
                xlib::XCloseDisplay(display);
                return Err(X11Error::ColorAlloc(foreground.into()));
            }

            // XOSD uses a 1-bit mask: black is transparent, white is visible.
            xlib::XSetForeground(display, mask_gc_back, xlib::XBlackPixel(display, screen));
            xlib::XSetBackground(display, mask_gc_back, xlib::XWhitePixel(display, screen));
            xlib::XSetForeground(display, mask_gc, xlib::XWhitePixel(display, screen));
            xlib::XSetBackground(display, mask_gc, xlib::XBlackPixel(display, screen));
            xlib::XSetForeground(display, gc, color.pixel);

            // The input shape is empty, making the OSD click-through.
            XShapeCombineRectangles(
                display,
                window,
                SHAPE_INPUT,
                0,
                0,
                ptr::null_mut(),
                0,
                SHAPE_SET,
                UNSORTED,
            );

            // Start hidden. render() installs the visible shape and maps it.
            XShapeCombineRectangles(
                display,
                window,
                SHAPE_BOUNDING,
                0,
                0,
                ptr::null_mut(),
                0,
                SHAPE_SET,
                UNSORTED,
            );

            xlib::XMapRaised(display, window);
            xlib::XFlush(display);

            Ok(Self {
                display,
                window,
                line_bitmap,
                mask_bitmap,
                gc,
                mask_gc,
                mask_gc_back,
                fontset,
                color,
                screen,
                width,
                height,
            })
        }
    }

    pub fn set_foreground(&mut self, foreground: &str) -> Result<(), X11Error> {
        unsafe {
            let colormap = xlib::XDefaultColormap(self.display, self.screen);
            let color_c = CString::new(foreground)
                .map_err(|_| X11Error::ColorAlloc(foreground.into()))?;
            let mut color: xlib::XColor = mem::zeroed();
            if xlib::XParseColor(self.display, colormap, color_c.as_ptr(), &mut color) == 0
                || xlib::XAllocColor(self.display, colormap, &mut color) == 0
            {
                return Err(X11Error::ColorAlloc(foreground.into()));
            }
            let old_pixel = self.color.pixel;
            self.color = color;
            xlib::XSetForeground(self.display, self.gc, color.pixel);
            let mut pixels = old_pixel;
            xlib::XFreeColors(self.display, colormap, &mut pixels, 1, 0);
            xlib::XFlush(self.display);
            Ok(())
        }
    }

    pub fn pump_events(&self) {
        unsafe {
            while xlib::XPending(self.display) > 0 {
                let mut event: xlib::XEvent = mem::zeroed();
                xlib::XNextEvent(self.display, &mut event);
            }
        }
    }

    fn clear_buffers(&self) {
        unsafe {
            xlib::XSetForeground(self.display, self.mask_gc_back, xlib::XBlackPixel(self.display, self.screen));
            xlib::XFillRectangle(
                self.display,
                self.mask_bitmap,
                self.mask_gc_back,
                0,
                0,
                self.width,
                self.height,
            );

            xlib::XSetForeground(self.display, self.gc, self.color.pixel);
            xlib::XFillRectangle(
                self.display,
                self.line_bitmap,
                self.gc,
                0,
                0,
                self.width,
                self.height,
            );
        }
    }
}

impl Renderer for X11Renderer {
    type Error = X11Error;

    fn render(&mut self, scene: &Scene) -> Result<(), Self::Error> {
        unsafe {
            self.clear_buffers();

            // Draw the same glyphs into both buffers. The mask therefore follows
            // the actual font rasterization rather than an approximate rectangle.
            for element in &scene.elements {
                let text = match CString::new(element.text.as_str()) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let bytes = text.as_bytes().len() as c_int;

                // XOSD measures the XFontSet text and right-aligns it within
                // the selected monitor, leaving XOFFSET=10 pixels.
                let mut ink = mem::zeroed::<xlib::XRectangle>();
                let mut logical = mem::zeroed::<xlib::XRectangle>();
                xlib::XmbTextExtents(
                    self.fontset,
                    text.as_ptr(),
                    bytes,
                    &mut ink,
                    &mut logical,
                );
                let text_width = logical.width as c_int;
                let x = (self.width as c_int - text_width - 10).max(0);
                let y = element.position.y.round() as c_int;

                xlib::XSetForeground(self.display, self.mask_gc, xlib::XWhitePixel(self.display, self.screen));
                xlib::XmbDrawString(
                    self.display,
                    self.mask_bitmap,
                    self.fontset,
                    self.mask_gc,
                    x,
                    y,
                    text.as_ptr(),
                    bytes,
                );

                xlib::XSetForeground(self.display, self.gc, self.color.pixel);
                xlib::XmbDrawString(
                    self.display,
                    self.line_bitmap,
                    self.fontset,
                    self.gc,
                    x,
                    y,
                    text.as_ptr(),
                    bytes,
                );
            }

            XShapeCombineMask(
                self.display,
                self.window,
                SHAPE_BOUNDING,
                0,
                0,
                self.mask_bitmap,
                SHAPE_SET,
            );

            // Copy the rendered screen-depth bitmap into the actual window.
            xlib::XCopyArea(
                self.display,
                self.line_bitmap,
                self.window,
                self.gc,
                0,
                0,
                self.width,
                self.height,
                0,
                0,
            );
            xlib::XFlush(self.display);
        }
        Ok(())
    }

    fn display_size(&self) -> Size {
        Size {
            width: self.width as f64,
            height: self.height as f64,
        }
    }
}

impl Drop for X11Renderer {
    fn drop(&mut self) {
        unsafe {
            xlib::XFreeGC(self.display, self.gc);
            xlib::XFreeGC(self.display, self.mask_gc);
            xlib::XFreeGC(self.display, self.mask_gc_back);
            xlib::XFreePixmap(self.display, self.line_bitmap);
            xlib::XFreePixmap(self.display, self.mask_bitmap);
            xlib::XFreeFontSet(self.display, self.fontset);
            xlib::XDestroyWindow(self.display, self.window);
            xlib::XCloseDisplay(self.display);
        }
    }
}
