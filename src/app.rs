//! The fullscreen window. An `NSTextView` does the editing, so input methods, paste,
//! undo and the emoji picker work without extra code. We only resize and place it.
//! While the cursor is hidden, a second, read-only view shows the formatted text instead.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use block2::RcBlock;
use dispatch2::{DispatchQueue, MainThreadBound};
use jiff::{Timestamp, Zoned};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    NSApplicationActivationPolicy, NSApplicationPresentationOptions,
    NSAttributedStringNSExtendedStringDrawing, NSBackingStoreType, NSColor, NSCursor, NSEvent,
    NSEventMask, NSEventModifierFlags, NSEventType, NSFont, NSFontAttributeName,
    NSFontFeatureSelectorIdentifierKey, NSFontFeatureSettingsAttribute,
    NSFontFeatureTypeIdentifierKey, NSFontManager, NSFontTraitMask, NSFontWeightRegular, NSMenu,
    NSMenuItem, NSScreen, NSStrikethroughStyleAttributeName, NSStringDrawingOptions,
    NSTextAlignment, NSTextDidChangeNotification, NSTextInputClient, NSTextInputTraitType,
    NSTextView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    NSActivityOptions, NSArray, NSAttributedString, NSDictionary, NSMutableAttributedString,
    NSNotificationCenter, NSNumber, NSPoint, NSProcessInfo, NSRect, NSSize, NSString, NSTimer,
};

use crate::layout::{self, Align, Padding, Size};
use crate::markdown::{self, Run};
use crate::placeholders::{self, Countdown};
use crate::stdin::Frames;
use crate::{Color, Config, Input};

/// How long the text cursor stays visible after the last keystroke or click.
const CURSOR_TIMEOUT: f64 = 1.5;

/// Font size used to measure text before scaling it to the screen.
const REFERENCE_SIZE: f64 = 100.0;

const ESCAPE_KEY_CODE: u16 = 53;

define_class!(
    // Borderless windows refuse key status by default, which would block typing.
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    struct KeyWindow;

    impl KeyWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            true
        }
    }
);

struct App {
    window: Retained<KeyWindow>,
    text_view: Retained<NSTextView>,
    display: Retained<NSTextView>,
    raw: bool,
    countdown: Option<Countdown>,
    /// The text last shown formatted, after filling in placeholders.
    shown: RefCell<String>,
    font_family: Option<String>,
    quarter_turns: u8,
    align: Align,
    padding: Padding,
    colors: RefCell<(Retained<NSColor>, Retained<NSColor>)>,
    cursor_timer: RefCell<Option<Retained<NSTimer>>>,
    /// Whether the user is typing, which shows the cursor and the unformatted source.
    cursor_visible: Cell<bool>,
}

pub fn run(config: Config) {
    let mtm = MainThreadMarker::new().expect("must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // A bare binary is a background app by default and never receives key events.
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    app.setMainMenu(Some(&main_menu(mtm)));

    let squint = Rc::new(App::new(&config, mtm));
    squint.install_event_monitor();
    squint.observe_edits();

    match config.input {
        // Without text, squint starts in typing mode, so the cursor shows where input goes.
        Input::Text(text) if text.is_empty() => squint.note_activity(),
        Input::Text(text) => {
            squint.text_view.setString(&NSString::from_str(&text));
            // As in sm, typing replaces text given as arguments.
            unsafe { squint.text_view.selectAll(None) };
        }
        Input::Stdin => read_stdin(Arc::new(MainThreadBound::new(squint.clone(), mtm))),
    }
    squint.relayout();
    squint.tick();

    squint.window.makeKeyAndOrderFront(None);
    // Cooperative activation (`NSApp.activate()`) is not granted when launched from a
    // terminal, so the window would open without keyboard focus.
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    app.setPresentationOptions(
        NSApplicationPresentationOptions::HideDock | NSApplicationPresentationOptions::HideMenuBar,
    );
    NSCursor::setHiddenUntilMouseMoves(true);
    // A sign is useless once the display sleeps. The activity lasts as long as squint runs.
    std::mem::forget(
        NSProcessInfo::processInfo().beginActivityWithOptions_reason(
            NSActivityOptions::IdleDisplaySleepDisabled | NSActivityOptions::UserInitiated,
            &NSString::from_str("Showing text fullscreen"),
        ),
    );
    app.run();
}

impl App {
    fn new(config: &Config, mtm: MainThreadMarker) -> Self {
        let screen = NSScreen::mainScreen(mtm).expect("no screen available");
        let frame = screen.frame();

        let window: Retained<KeyWindow> = unsafe {
            msg_send![
                KeyWindow::alloc(mtm),
                initWithContentRect: frame,
                styleMask: NSWindowStyleMask::Borderless,
                backing: NSBackingStoreType::Buffered,
                defer: false,
            ]
        };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setFrame_display(frame, true);

        let text_view = new_text_view(frame, mtm);
        text_view.setRichText(false);
        text_view.setAllowsUndo(true);
        text_view.setAlignment(match config.align {
            Align::Center => NSTextAlignment::Center,
            Align::Left => NSTextAlignment::Left,
            Align::Right => NSTextAlignment::Right,
        });
        // The text is shown verbatim, so anything that rewrites or annotates it stays off.
        text_view.setAutomaticQuoteSubstitutionEnabled(false);
        text_view.setAutomaticDashSubstitutionEnabled(false);
        text_view.setAutomaticTextReplacementEnabled(false);
        text_view.setAutomaticSpellingCorrectionEnabled(false);
        text_view.setContinuousSpellCheckingEnabled(false);
        text_view.setGrammarCheckingEnabled(false);
        text_view.setAutomaticTextCompletionEnabled(false);
        text_view.setInlinePredictionType(NSTextInputTraitType::No);

        let display = new_text_view(frame, mtm);
        display.setEditable(false);
        display.setSelectable(false);

        let content = window.contentView().expect("window has a content view");
        content.addSubview(&display);
        content.addSubview(&text_view);
        window.makeFirstResponder(Some(&text_view));

        let app = App {
            window,
            text_view,
            display,
            raw: config.raw,
            countdown: config.countdown.clone(),
            shown: RefCell::new(String::new()),
            font_family: config.font.clone(),
            quarter_turns: config.quarter_turns,
            align: config.align,
            padding: config.padding,
            colors: RefCell::new((ns_color(&config.foreground), ns_color(&config.background))),
            cursor_timer: RefCell::new(None),
            cursor_visible: Cell::new(false),
        };
        if let Some(family) = &config.font
            && app.font(REFERENCE_SIZE).is_none()
        {
            eprintln!("squint: font \"{family}\" not found, using the system font");
        }
        app.apply_colors();
        app
    }

    fn font_or_system(&self, size: f64) -> Retained<NSFont> {
        self.font(size)
            .unwrap_or_else(|| NSFont::systemFontOfSize(size))
    }

    fn font(&self, size: f64) -> Option<Retained<NSFont>> {
        let Some(family) = &self.font_family else {
            return Some(NSFont::systemFontOfSize(size));
        };
        let name = NSString::from_str(family);
        let mtm = MainThreadMarker::from(&*self.text_view);
        NSFontManager::sharedFontManager(mtm)
            .fontWithFamily_traits_weight_size(&name, NSFontTraitMask(0), 5, size)
            .or_else(|| NSFont::fontWithName_size(&name, size))
    }

    /// Shows the source while typing and the formatted text otherwise, each scaled to fill
    /// the screen.
    fn relayout(&self) {
        let editing = self.cursor_visible.get();
        // The editor stays first responder while invisible, so typing switches back to it.
        self.text_view
            .setAlphaValue(if editing { 1.0 } else { 0.0 });
        self.display.setHidden(editing);

        let source = self.text_view.string().to_string();
        if editing {
            let mut text = source;
            // A trailing empty line, where the cursor sits after Return, needs room too.
            if text.is_empty() || text.ends_with('\n') {
                text.push('M');
            }
            let size = self.place(&self.text_view, |size| {
                plain(&text, &self.font_or_system(size))
            });
            self.text_view.setFont(Some(&self.font_or_system(size)));
        } else {
            let text = placeholders::fill(&source, &Zoned::now(), self.countdown.as_ref());
            let runs = if self.raw {
                vec![Run {
                    text: text.clone(),
                    style: markdown::PLAIN,
                }]
            } else {
                markdown::parse(&text)
            };
            self.shown.replace(text);
            let storage = unsafe { self.display.textStorage() }.expect("text view has storage");
            if runs.is_empty() {
                storage.setAttributedString(&NSAttributedString::new());
            } else {
                let size = self.place(&self.display, |size| self.styled(&runs, size));
                storage.setAttributedString(&self.styled(&runs, size));
                self.display.setTextColor(Some(&self.colors.borrow().0));
                self.display.setAlignment(self.text_view.alignment());
            }
        }
        // Rotated views leave stale pixels behind when they move, so repaint everything.
        self.window.contentView().unwrap().setNeedsDisplay(true);
    }

    /// Redraws once a second while placeholders change the shown text.
    fn tick(self: &Rc<Self>) {
        if !self.cursor_visible.get() {
            let source = self.text_view.string().to_string();
            let text = placeholders::fill(&source, &Zoned::now(), self.countdown.as_ref());
            if *self.shown.borrow() != text {
                self.relayout();
            }
        }
        let app = self.clone();
        let block = RcBlock::new(move |_: NonNull<NSTimer>| app.tick());
        // Fire a moment after the countdown's next full second, or the clock's without one.
        let phase = self
            .countdown
            .as_ref()
            .map_or(0, |c| c.end.subsec_nanosecond());
        let wait = (phase - Timestamp::now().subsec_nanosecond()).rem_euclid(1_000_000_000);
        let delay = f64::from(wait) / 1e9 + 0.001;
        unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(delay, false, &block) };
    }

    /// Picks the font size at which `build` makes text that fills the screen, and places
    /// `view` to show that text. Returns the font size.
    fn place(&self, view: &NSTextView, build: impl Fn(f64) -> Retained<NSAttributedString>) -> f64 {
        let frame = self.window.frame();
        // On notched displays, the strip beside the camera is unusable for text.
        let notch = self.window.screen().map_or(0.0, |s| s.safeAreaInsets().top);
        let ((left, bottom), screen) = self.padding.inset(Size {
            width: frame.size.width,
            height: frame.size.height - notch,
        });

        let reference = measure(&build(REFERENCE_SIZE));
        let size = layout::fit_font_size(REFERENCE_SIZE, reference, screen, self.quarter_turns);
        let measured = measure(&build(size));

        if let Some(container) = unsafe { view.textContainer() } {
            // Slightly wider than measured, so rounding never wraps a line.
            container.setSize(NSSize::new(measured.width + 1.0, f64::MAX));
        }
        let (x, y) = layout::text_origin(measured, screen, self.quarter_turns, self.align);
        // Rotation applies around the view's center, so set the unrotated frame first.
        view.setFrameCenterRotation(0.0);
        view.setFrame(NSRect::new(
            NSPoint::new(left + x, bottom + y),
            NSSize::new(measured.width + 1.0, measured.height),
        ));
        // AppKit rotates counterclockwise, sm clockwise.
        view.setFrameCenterRotation(-90.0 * f64::from(self.quarter_turns));
        size
    }

    /// Formatted text with body text at `size` points.
    fn styled(&self, runs: &[Run], size: f64) -> Retained<NSAttributedString> {
        let mtm = MainThreadMarker::from(&*self.text_view);
        let fonts = NSFontManager::sharedFontManager(mtm);
        let result = NSMutableAttributedString::new();
        for run in runs {
            let style = run.style;
            let size = size * style.scale;
            let mut font = if style.code {
                NSFont::monospacedSystemFontOfSize_weight(size, unsafe { NSFontWeightRegular })
            } else {
                self.font_or_system(size)
            };
            if style.bold {
                font = fonts.convertFont_toHaveTrait(&font, NSFontTraitMask::BoldFontMask);
            }
            if style.italic {
                font = fonts.convertFont_toHaveTrait(&font, NSFontTraitMask::ItalicFontMask);
            }
            let font = tabular_digits(&font);
            let mut keys = vec![unsafe { NSFontAttributeName }];
            let mut values = vec![Retained::into_super(Retained::into_super(font))];
            if style.strikethrough {
                keys.push(unsafe { NSStrikethroughStyleAttributeName });
                values.push(Retained::into_super(Retained::into_super(
                    Retained::into_super(NSNumber::new_i32(1)),
                )));
            }
            let attributes = NSDictionary::from_retained_objects(&keys, &values);
            let text = unsafe {
                NSAttributedString::new_with_attributes(&NSString::from_str(&run.text), &attributes)
            };
            result.appendAttributedString(&text);
        }
        Retained::into_super(result)
    }

    fn show_frame(&self, text: &str) {
        self.text_view.setString(&NSString::from_str(text));
        // Undo steps from typing refer to text that is gone now.
        if let Some(undo) = self.text_view.undoManager() {
            undo.removeAllActions();
        }
        self.relayout();
    }

    fn apply_colors(&self) {
        let (foreground, background) = &*self.colors.borrow();
        self.window.setBackgroundColor(Some(background));
        self.text_view.setTextColor(Some(foreground));
        self.display.setTextColor(Some(foreground));
        self.set_cursor_visible(self.cursor_visible.get());
    }

    fn invert(&self) {
        {
            let mut colors = self.colors.borrow_mut();
            let (foreground, background) = &mut *colors;
            std::mem::swap(foreground, background);
        }
        self.apply_colors();
    }

    /// Shows or hides the insertion point and the selection highlight, and switches between
    /// the source and the formatted text.
    fn set_cursor_visible(&self, visible: bool) {
        if self.cursor_visible.replace(visible) != visible {
            self.relayout();
        }
        let clear = NSColor::clearColor();
        let foreground = &self.colors.borrow().0;
        self.text_view
            .setInsertionPointColor(Some(if visible { foreground } else { &clear }));
        let highlight = if visible {
            NSColor::selectedTextBackgroundColor()
        } else {
            clear
        };
        let attributes = NSDictionary::from_retained_objects(
            &[unsafe { objc2_app_kit::NSBackgroundColorAttributeName }],
            &[Retained::into_super(Retained::into_super(highlight))],
        );
        unsafe { self.text_view.setSelectedTextAttributes(&attributes) };
    }

    /// Shows the cursor and restarts the countdown that hides it again.
    fn note_activity(self: &Rc<Self>) {
        self.set_cursor_visible(true);
        if let Some(timer) = self.cursor_timer.take() {
            timer.invalidate();
        }
        let app = self.clone();
        let block = RcBlock::new(move |_: NonNull<NSTimer>| {
            app.set_cursor_visible(false);
        });
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(CURSOR_TIMEOUT, false, &block)
        };
        self.cursor_timer.replace(Some(timer));
    }

    /// Handles sm's shortcuts before the text view sees them, and tracks activity.
    fn install_event_monitor(self: &Rc<Self>) {
        let app = self.clone();
        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let event_ref = unsafe { event.as_ref() };
            app.note_activity();
            if event_ref.r#type() == NSEventType::KeyDown && app.handle_key(event_ref) {
                return std::ptr::null_mut();
            }
            event.as_ptr()
        });
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::KeyDown | NSEventMask::LeftMouseDown,
                &block,
            )
        };
        // The monitor lives as long as the app.
        std::mem::forget(monitor);
    }

    /// Returns whether the key was consumed.
    fn handle_key(&self, event: &NSEvent) -> bool {
        let control = event
            .modifierFlags()
            .contains(NSEventModifierFlags::Control);
        let key = event
            .charactersIgnoringModifiers()
            .map(|s| s.to_string().to_lowercase());
        let mtm = MainThreadMarker::from(&*self.text_view);
        match (control, key.as_deref()) {
            (true, Some("q")) => NSApplication::sharedApplication(mtm).terminate(None),
            (true, Some("i")) => self.invert(),
            // Esc also cancels input method composition, which must keep working.
            _ if event.keyCode() == ESCAPE_KEY_CODE && !self.text_view.hasMarkedText() => {
                // Holding Esc would otherwise clear and quit in one go.
                if event.isARepeat() {
                    return true;
                }
                if self.text_view.string().length() == 0 {
                    NSApplication::sharedApplication(mtm).terminate(None);
                }
                // Clear through the text view's own action, so Cmd-Z restores the text.
                unsafe {
                    self.text_view.selectAll(None);
                    self.text_view.delete(None);
                }
            }
            _ => return false,
        }
        true
    }

    fn observe_edits(self: &Rc<Self>) {
        let app = self.clone();
        let block = RcBlock::new(move |_| app.relayout());
        let observer = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSTextDidChangeNotification),
                Some(&self.text_view),
                None,
                &block,
            )
        };
        std::mem::forget::<Retained<ProtocolObject<_>>>(observer);
    }
}

fn read_stdin(app: Arc<MainThreadBound<Rc<App>>>) {
    // Holds the newest frame not shown yet. Frames arriving faster than they can be
    // drawn replace each other here instead of queueing up on the main thread.
    let pending = Arc::new(Mutex::new(None::<String>));
    let show = move |text: String| {
        if pending.lock().unwrap().replace(text).is_some() {
            return;
        }
        let (app, pending) = (app.clone(), pending.clone());
        DispatchQueue::main().exec_async(move || {
            let mtm = MainThreadMarker::new().unwrap();
            if let Some(text) = pending.lock().unwrap().take() {
                app.get(mtm).show_frame(&text);
            }
        });
    };
    std::thread::spawn(move || {
        let mut frames = Frames::default();
        let mut stdin = std::io::stdin().lock();
        let mut buffer = [0; 4096];
        loop {
            match stdin.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if let Some(text) = frames.push(&buffer[..n]) {
                        show(text);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    eprintln!("squint: reading stdin failed: {e}");
                    break;
                }
            }
        }
        if let Some(text) = frames.finish() {
            show(text);
        }
    });
}

/// A text view that only shows what it is given, positioned and sized by `App::place`.
fn new_text_view(frame: NSRect, mtm: MainThreadMarker) -> Retained<NSTextView> {
    let view = NSTextView::initWithFrame(NSTextView::alloc(mtm), frame);
    view.setDrawsBackground(false);
    view.setHorizontallyResizable(false);
    view.setVerticallyResizable(false);
    view.setTextContainerInset(NSSize::ZERO);
    if let Some(container) = unsafe { view.textContainer() } {
        container.setWidthTracksTextView(false);
        container.setHeightTracksTextView(false);
        container.setLineFragmentPadding(0.0);
    }
    view
}

/// The font with digits of equal width, so ticking numbers keep the text the same size.
fn tabular_digits(font: &NSFont) -> Retained<NSFont> {
    // From Apple's SFNTLayoutTypes.h: kNumberSpacingType, kMonospacedNumbersSelector.
    let feature = NSDictionary::from_retained_objects(
        &[unsafe { NSFontFeatureTypeIdentifierKey }, unsafe {
            NSFontFeatureSelectorIdentifierKey
        }],
        &[NSNumber::new_i32(6), NSNumber::new_i32(0)],
    );
    let features = NSArray::from_retained_slice(&[feature]);
    let attributes = NSDictionary::from_retained_objects(
        &[unsafe { NSFontFeatureSettingsAttribute }],
        &[Retained::into_super(Retained::into_super(features))],
    );
    let descriptor = unsafe {
        font.fontDescriptor()
            .fontDescriptorByAddingAttributes(&attributes)
    };
    NSFont::fontWithDescriptor_size(&descriptor, font.pointSize()).unwrap_or_else(|| font.retain())
}

fn plain(text: &str, font: &NSFont) -> Retained<NSAttributedString> {
    let attributes = NSDictionary::from_retained_objects(
        &[unsafe { NSFontAttributeName }],
        &[Retained::into_super(Retained::into_super(font.retain()))],
    );
    unsafe { NSAttributedString::new_with_attributes(&NSString::from_str(text), &attributes) }
}

fn measure(string: &NSAttributedString) -> Size {
    let rect = string.boundingRectWithSize_options_context(
        NSSize::new(f64::MAX, f64::MAX),
        NSStringDrawingOptions::UsesLineFragmentOrigin,
        None,
    );
    Size {
        width: rect.size.width.ceil(),
        height: rect.size.height.ceil(),
    }
}

fn ns_color(color: &Color) -> Retained<NSColor> {
    match color {
        Color::Text => by_appearance(NSColor::blackColor(), NSColor::whiteColor()),
        Color::Background => by_appearance(NSColor::whiteColor(), NSColor::blackColor()),
        Color::Custom(c) => NSColor::colorWithSRGBRed_green_blue_alpha(
            c.r.into(),
            c.g.into(),
            c.b.into(),
            c.a.into(),
        ),
    }
}

/// A color AppKit resolves on every draw, so it follows appearance changes while running.
fn by_appearance(light: Retained<NSColor>, dark: Retained<NSColor>) -> Retained<NSColor> {
    let provider = RcBlock::new(move |appearance: NonNull<NSAppearance>| {
        let names = NSArray::from_slice(&[unsafe { NSAppearanceNameAqua }, unsafe {
            NSAppearanceNameDarkAqua
        }]);
        let best = unsafe { appearance.as_ref() }.bestMatchFromAppearancesWithNames(&names);
        let is_dark = best.is_some_and(|name| &*name == unsafe { NSAppearanceNameDarkAqua });
        // The block owns both colors, so the returned pointer stays valid.
        NonNull::from(if is_dark { &*dark } else { &*light })
    });
    unsafe { NSColor::colorWithName_dynamicProvider(None, &provider) }
}

/// Without a main menu, AppKit has nowhere to look up Cmd-Q, Cmd-V and friends.
fn main_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let item = |title: &str, action, key: &str| unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::from_str(key),
        )
    };
    let submenu = |items: &[Retained<NSMenuItem>]| {
        let menu = NSMenu::new(mtm);
        for item in items {
            menu.addItem(item);
        }
        let parent = NSMenuItem::new(mtm);
        parent.setSubmenu(Some(&menu));
        parent
    };

    let menu = NSMenu::new(mtm);
    menu.addItem(&submenu(&[item("Quit squint", sel!(terminate:), "q")]));
    menu.addItem(&submenu(&[
        item("Undo", sel!(undo:), "z"),
        item("Redo", sel!(redo:), "Z"),
        item("Cut", sel!(cut:), "x"),
        item("Copy", sel!(copy:), "c"),
        item("Paste", sel!(paste:), "v"),
        item("Select All", sel!(selectAll:), "a"),
    ]));
    menu
}
