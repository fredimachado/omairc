use crate::convert::to_iced;
use crate::fonts::{bold, IA_WRITER};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::text::{self, Paragraph as TextParagraph};
use iced::advanced::widget::text::draw as draw_text;
use iced::advanced::widget::{operation, tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::alignment::Vertical;
use iced::keyboard;
use iced::mouse;
use iced::widget::text::Span;
use iced::{
    Color, Element, Event, Length, Pixels, Point, Rectangle, Size, Theme, Vector,
};
use iced::advanced::widget::operation::Focusable;
use lru::LruCache;
use shared::{
    map_pointer, FormattedText, PointerZone, Pos, Row, RowKind, ScrollAnchor, Style,
    TextStyle, Transcript,
};
use std::cell::RefCell;
use std::num::NonZeroUsize;
use std::ops::Range;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum TranscriptMessage {
    Select(SelectKind),
    ScrollTo(ScrollAnchor),
    Measured(usize, i32),
    Copy,
    LinkClick(String),
    FocusChanged(bool),
    DragScrollActive(bool),
    DragEdge(Option<i8>),
}

#[derive(Debug, Clone, Copy)]
pub enum SelectKind {
    Begin(Pos),
    Extend(Pos),
    Word(Pos),
    Row(usize),
    Clear,
    All,
}

pub struct TranscriptWidget<'a> {
    pub transcript: &'a Transcript,
    pub style: &'a Style,
    pub theme_generation: u64,
    pub member_realnames: &'a std::collections::HashMap<String, String>,
    pub shift_held: bool,
    id: iced::widget::Id,
}

impl<'a> TranscriptWidget<'a> {
    pub fn new(
        transcript: &'a Transcript,
        style: &'a Style,
        theme_generation: u64,
        member_realnames: &'a std::collections::HashMap<String, String>,
        shift_held: bool,
    ) -> Self {
        Self {
            transcript,
            style,
            theme_generation,
            member_realnames,
            shift_held,
            id: iced::widget::Id::unique(),
        }
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct CacheKey {
    row_id: u64,
    width_bits: u32,
    scale_bits: u32,
    theme_gen: u64,
    centered: bool,
}

struct WidgetState<P> {
    paragraphs: RefCell<LruCache<CacheKey, P>>,
    dragging: bool,
    last_click: Option<(Instant, usize, u8)>,
    hovered_link: Option<(usize, usize)>,
    focused: bool,
    pending_measure: Vec<(usize, i32)>,
}

impl<P> Default for WidgetState<P> {
    fn default() -> Self {
        Self {
            paragraphs: RefCell::new(LruCache::new(NonZeroUsize::new(512).unwrap())),
            dragging: false,
            last_click: None,
            hovered_link: None,
            focused: false,
            pending_measure: Vec::new(),
        }
    }
}

impl<P> Focusable for WidgetState<P> {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
    }
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

fn row_height_at(transcript: &Transcript, row: usize) -> i32 {
    if row + 1 < transcript.rows().len() {
        transcript.y_of_row(row + 1) - transcript.y_of_row(row)
    } else if row > 0 {
        transcript.y_of_row(row) - transcript.y_of_row(row - 1)
    } else {
        72
    }
}

fn scroll_top(transcript: &Transcript, viewport_h: i32) -> i32 {
    let total: i32 = (0..transcript.rows().len())
        .map(|i| row_height_at(transcript, i))
        .sum();
    let max_top = (total - viewport_h).max(0);
    if transcript.scroll().pinned_to_bottom {
        max_top
    } else {
        (transcript.y_of_row(transcript.scroll().row) + transcript.scroll().offset_px)
            .clamp(0, max_top)
    }
}

fn body_width(total_width: f32, style: &Style) -> f32 {
    let left = style.scaled(70.0) as f32;
    let right = style.scaled(34.0) as f32;
    (total_width - left - right).max(1.0)
}

#[derive(Clone)]
struct ParagraphOverlay {
    selection: Option<Range<usize>>,
    hover_link: Option<Range<usize>>,
    selection_color: Color,
}

impl Default for ParagraphOverlay {
    fn default() -> Self {
        Self {
            selection: None,
            hover_link: None,
            selection_color: Color::TRANSPARENT,
        }
    }
}

impl ParagraphOverlay {
    fn cached(&self) -> bool {
        self.selection.is_none() && self.hover_link.is_none()
    }
}

fn hovered_link_range(
    state: &WidgetState<impl Clone>,
    row_index: usize,
    row: &Row,
) -> Option<Range<usize>> {
    let (r, i) = state.hovered_link?;
    if r != row_index {
        return None;
    }
    row.body.links.get(i).cloned()
}

fn build_spans(
    formatted: &FormattedText,
    style: &Style,
    font_size: f32,
    default_ink: Color,
    selection: Option<Range<usize>>,
    selection_color: Color,
    hover_link: Option<Range<usize>>,
) -> Vec<Span<'static, (), iced::Font>> {
    let plain = &formatted.plain;
    if plain.is_empty() {
        return vec![];
    }
    let mut boundaries = vec![0, plain.len()];
    for run in &formatted.runs {
        boundaries.push(run.range.start);
        boundaries.push(run.range.end);
    }
    for link in &formatted.links {
        boundaries.push(link.start);
        boundaries.push(link.end);
    }
    if let Some(sel) = &selection {
        boundaries.push(sel.start);
        boundaries.push(sel.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut spans = Vec::new();
    for window in boundaries.windows(2) {
        let (start, end) = (window[0], window[1]);
        if start >= end {
            continue;
        }
        let slice = plain[start..end].to_string();
        let mut ts = TextStyle::default();
        for run in &formatted.runs {
            if run.range.start <= start && end <= run.range.end {
                ts = run.style.clone();
            }
        }
        let mut color = default_ink;
        if let Some(fg) = ts.fg {
            color = to_iced(fg);
        }
        let is_link = formatted.links.iter().any(|l| l.start <= start && end <= l.end);
        let selected = selection
            .as_ref()
            .is_some_and(|sel| start >= sel.start && end <= sel.end);
        let link_hovered = hover_link
            .as_ref()
            .is_some_and(|l| start >= l.start && end <= l.end);
        let mut span = Span::new(slice).color(color).size(Pixels(font_size));
        if ts.bold {
            span = span.font(bold());
        }
        if selected {
            span = span
                .color(Color::WHITE)
                .background(iced::Background::Color(selection_color));
        }
        if ts.underline || (is_link && link_hovered) {
            span = span.underline(true);
        }
        if ts.strikethrough {
            span = span.strikethrough(true);
        }
        spans.push(span);
    }
    if spans.is_empty() {
        spans.push(
            Span::new(plain.clone())
                .color(default_ink)
                .size(Pixels(font_size)),
        );
    }
    spans
}

fn paragraph_for_row<Renderer: renderer::Renderer + text::Renderer<Font = iced::Font>>(
    row: &Row,
    style: &Style,
    body_w: f32,
    theme_gen: u64,
    centered: bool,
    state: &WidgetState<Renderer::Paragraph>,
    renderer: &Renderer,
    font_size: f32,
    overlay: ParagraphOverlay,
) -> Renderer::Paragraph
where
    Renderer::Paragraph: TextParagraph + Clone + Default,
{
    let key = CacheKey {
        row_id: row.id,
        width_bits: body_w.to_bits(),
        scale_bits: (style.text_scale * 1000.0).round() as u32,
        theme_gen,
        centered,
    };
    let cacheable = overlay.cached();
    if cacheable {
        if let Some(p) = state.paragraphs.borrow_mut().get(&key) {
            return p.clone();
        }
    }

    let ParagraphOverlay {
        selection,
        hover_link,
        selection_color,
    } = overlay;

    let ink = to_iced(style.ink_color());
    let muted = to_iced(style.muted_color());
    let paragraph = if centered {
        let content = match row.kind {
            RowKind::DaySeparator => row
                .separator_text
                .as_deref()
                .unwrap_or(&row.body.plain)
                .to_string(),
            RowKind::Event => row
                .event_text
                .as_deref()
                .unwrap_or(&row.body.plain)
                .to_string(),
            RowKind::Message => row.body.plain.clone(),
        };
        Renderer::Paragraph::with_text(text::Text {
            content: &content,
            bounds: Size::new(body_w, f32::INFINITY),
            size: Pixels(font_size),
            line_height: text::LineHeight::default(),
            font: IA_WRITER,
            align_x: text::Alignment::Center,
            align_y: Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::Word,
        })
    } else {
        let spans = build_spans(
            &row.body,
            style,
            font_size,
            ink,
            selection,
            selection_color,
            hover_link,
        );
        Renderer::Paragraph::with_spans(text::Text {
            content: spans.as_slice(),
            bounds: Size::new(body_w, f32::INFINITY),
            size: Pixels(font_size),
            line_height: text::LineHeight::default(),
            font: IA_WRITER,
            align_x: text::Alignment::Left,
            align_y: Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::Word,
        })
    };
    let _ = (muted, renderer);
    if cacheable {
        state.paragraphs.borrow_mut().put(key, paragraph.clone());
    }
    paragraph
}

fn row_height<Renderer: renderer::Renderer + text::Renderer<Font = iced::Font>>(
    row: &Row,
    style: &Style,
    width: f32,
    theme_gen: u64,
    state: &WidgetState<Renderer::Paragraph>,
    renderer: &Renderer,
) -> i32
where
    Renderer::Paragraph: TextParagraph + Clone + Default,
{
    if row.kind == RowKind::DaySeparator || row.kind == RowKind::Event {
        let p = paragraph_for_row(
            row,
            style,
            width - style.scaled(40.0) as f32,
            theme_gen,
            true,
            state,
            renderer,
            style.scaled(10.0) as f32,
            ParagraphOverlay::default(),
        );
        return (p.min_bounds().height as i32 + style.scaled(16.0)).max(style.scaled(24.0));
    }
    let header = if row.kind == RowKind::Message && !row.continues {
        style.scaled(18.0)
    } else {
        0
    };
    let p = paragraph_for_row(
        row,
        style,
        body_width(width, style),
        theme_gen,
        false,
        state,
        renderer,
        style.scaled(13.0) as f32,
        ParagraphOverlay::default(),
    );
    let body = p.min_bounds().height as i32 + style.scaled(8.0);
    body.max(style.scaled(52.0) + header).max(style.scaled(40.0))
}

impl<'a, Message, Renderer> Widget<Message, Theme, Renderer> for TranscriptWidget<'a>
where
    Message: From<TranscriptMessage> + Clone + 'static,
    Renderer: renderer::Renderer + text::Renderer<Font = iced::Font>,
    Renderer::Paragraph: TextParagraph<Font = iced::Font> + Clone + Default,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<WidgetState<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(WidgetState::<Renderer::Paragraph>::default())
    }

    fn diff(&self, _tree: &mut Tree) {}

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<WidgetState<Renderer::Paragraph>>();
        let width = limits.max().width;
        let viewport_h = limits.max().height.max(1.0) as i32;
        let range = self
            .transcript
            .visible_range(viewport_h, self.style.scaled(120.0));
        state.pending_measure.clear();
        for i in range {
            if i >= self.transcript.rows().len() {
                continue;
            }
            let row = &self.transcript.rows()[i];
            let h = row_height(row, self.style, width, self.theme_generation, state, renderer);
            state.pending_measure.push((i, h));
        }
        layout::atomic(limits, Length::Fill, Length::Fill)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn operation::Operation,
    ) {
        let state = tree
            .state
            .downcast_mut::<WidgetState<Renderer::Paragraph>>();
        operation.focusable(Some(&self.id), layout.bounds(), state);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<WidgetState<Renderer::Paragraph>>();
        for (row, h) in state.pending_measure.drain(..) {
            shell.publish(TranscriptMessage::Measured(row, h).into());
        }

        let bounds = layout.bounds();
        if !bounds.intersects(viewport) {
            return;
        }
        let width = bounds.width;
        let viewport_h = bounds.height as i32;
        let top = scroll_top(self.transcript, viewport_h);

        if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
            if state.focused {
                use iced::keyboard::{key::Named, Key};
                let key = key.as_ref();
                if modifiers.control() && matches!(key, Key::Character("a") | Key::Character("A")) {
                    shell.publish(TranscriptMessage::Select(SelectKind::All).into());
                    shell.capture_event();
                    return;
                }
                if modifiers.control() && matches!(key, Key::Character("c") | Key::Character("C")) {
                    shell.publish(TranscriptMessage::Copy.into());
                    shell.capture_event();
                    return;
                }
                if matches!(key, Key::Named(Named::Escape)) {
                    shell.publish(TranscriptMessage::Select(SelectKind::Clear).into());
                    shell.capture_event();
                    return;
                }
            }
        }

        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event {
            let step = match delta {
                mouse::ScrollDelta::Lines { y, .. } => (-y * 40.0) as i32,
                mouse::ScrollDelta::Pixels { y, .. } => (-y) as i32,
            };
            let row = self.transcript.scroll().row;
            let new_off = self.transcript.scroll().offset_px + step;
            shell.publish(
                TranscriptMessage::ScrollTo(ScrollAnchor {
                    row,
                    offset_px: new_off,
                    pinned_to_bottom: false,
                })
                .into(),
            );
            shell.capture_event();
            return;
        }

        let position = cursor.position_in(bounds);
        let mut next_hover = None;
        if let Some(pos) = position {
            let content_y = pos.y - bounds.y + top as f32;
            let row_index = self.transcript.row_at_y(content_y as i32);
            if row_index < self.transcript.rows().len() {
                let row = &self.transcript.rows()[row_index];
                let row_y = self.transcript.y_of_row(row_index) as f32;
                if pointer_in_body(row, row_y, content_y, pos.x - bounds.x, self.style) {
                    let body_left = self.style.scaled(70.0) as f32;
                    let body_w = body_width(width, self.style);
                    let header_off = message_header_offset(row, self.style);
                    let p = paragraph_for_row(
                        row,
                        self.style,
                        body_w,
                        self.theme_generation,
                        false,
                        state,
                        renderer,
                        self.style.scaled(13.0) as f32,
                        ParagraphOverlay::default(),
                    );
                    if let Some(text::Hit::CharOffset(offset)) = p.hit_test(Point::new(
                        pos.x - bounds.x - body_left,
                        content_y - row_y - header_off,
                    )) {
                        for (i, link) in row.body.links.iter().enumerate() {
                            if link.start <= offset && offset <= link.end {
                                next_hover = Some((row_index, i));
                                break;
                            }
                        }
                    }
                }
            }
        }
        if state.hovered_link != next_hover {
            state.hovered_link = next_hover;
            shell.request_redraw();
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = position {
                    state.focused = true;
                    shell.publish(TranscriptMessage::FocusChanged(true).into());
                    let content_y = pos.y - bounds.y + top as f32;
                    let row_index = self.transcript.row_at_y(content_y as i32);
                    if row_index >= self.transcript.rows().len() {
                        return;
                    }
                    let row = &self.transcript.rows()[row_index];
                    let row_y = self.transcript.y_of_row(row_index) as f32;
                    let lx = pos.x - bounds.x;

                    if let Some(zone) = pointer_zone(row, row_y, content_y, lx, self.style) {
                        let mapped = map_pointer(self.transcript.rows(), row_index, zone);
                        if self.shift_held {
                            shell.publish(TranscriptMessage::Select(SelectKind::Extend(mapped)).into());
                        } else {
                            shell.publish(TranscriptMessage::Select(SelectKind::Begin(mapped)).into());
                        }
                    } else {
                        let body_left = self.style.scaled(70.0) as f32;
                        let body_w = body_width(width, self.style);
                        let header_off = message_header_offset(row, self.style);
                        let p = paragraph_for_row(
                            row,
                            self.style,
                            body_w,
                            self.theme_generation,
                            false,
                            state,
                            renderer,
                            self.style.scaled(13.0) as f32,
                            ParagraphOverlay::default(),
                        );
                        let offset = p
                            .hit_test(Point::new(lx - body_left, content_y - row_y - header_off))
                            .and_then(|h| match h {
                                text::Hit::CharOffset(o) => Some(o),
                            })
                            .unwrap_or(0);
                        let click_pos = Pos {
                            row: row_index,
                            offset,
                        };
                        let now = Instant::now();
                        let triple = state.last_click.is_some_and(|(t, r, c)| {
                            r == row_index && c >= 2 && now - t < Duration::from_millis(400)
                        });
                        let double = !triple
                            && state.last_click.is_some_and(|(t, r, c)| {
                                r == row_index && c == 1 && now - t < Duration::from_millis(400)
                            });

                        if triple {
                            shell.publish(TranscriptMessage::Select(SelectKind::Row(row_index)).into());
                        } else if double {
                            shell.publish(TranscriptMessage::Select(SelectKind::Word(click_pos)).into());
                        } else if self.shift_held {
                            shell.publish(TranscriptMessage::Select(SelectKind::Extend(click_pos)).into());
                        } else {
                            shell.publish(TranscriptMessage::Select(SelectKind::Begin(click_pos)).into());
                        }
                        state.last_click = Some((
                            now,
                            row_index,
                            if triple { 3 } else if double { 2 } else { 1 },
                        ));
                    }
                    state.dragging = true;
                    shell.publish(TranscriptMessage::DragScrollActive(true).into());
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging {
                    state.dragging = false;
                    shell.publish(TranscriptMessage::DragScrollActive(false).into());
                }
                if let Some((row_index, link_idx)) = state.hovered_link {
                    let row = &self.transcript.rows()[row_index];
                    if let Some(link) = row.body.links.get(link_idx) {
                        let url = row.body.plain[link.clone()].to_string();
                        shell.publish(TranscriptMessage::LinkClick(url).into());
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                if let Some(pos) = position {
                    let edge = if pos.y < bounds.y + 20.0 {
                        Some(-1i8)
                    } else if pos.y > bounds.y + bounds.height - 20.0 {
                        Some(1)
                    } else {
                        None
                    };
                    shell.publish(TranscriptMessage::DragEdge(edge).into());
                    let content_y = pos.y - bounds.y + top as f32;
                    let row_index = self.transcript.row_at_y(content_y as i32);
                    if row_index < self.transcript.rows().len() {
                        let row = &self.transcript.rows()[row_index];
                        let row_y = self.transcript.y_of_row(row_index) as f32;
                        let lx = pos.x - bounds.x;
                        if pointer_zone(row, row_y, content_y, lx, self.style).is_none() {
                            let body_left = self.style.scaled(70.0) as f32;
                            let body_w = body_width(width, self.style);
                            let header_off = message_header_offset(row, self.style);
                            let p = paragraph_for_row(
                                row,
                                self.style,
                                body_w,
                                self.theme_generation,
                                false,
                                state,
                                renderer,
                                self.style.scaled(13.0) as f32,
                                ParagraphOverlay::default(),
                            );
                            let offset = p
                                .hit_test(Point::new(lx - body_left, content_y - row_y - header_off))
                                .and_then(|h| match h {
                                    text::Hit::CharOffset(o) => Some(o),
                                })
                                .unwrap_or(0);
                            shell.publish(
                                TranscriptMessage::Select(SelectKind::Extend(Pos {
                                    row: row_index,
                                    offset,
                                }))
                                .into(),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree
            .state
            .downcast_ref::<WidgetState<Renderer::Paragraph>>();
        let bounds = layout.bounds();
        if !bounds.intersects(viewport) {
            return;
        }

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                ..Default::default()
            },
            iced::Background::Color(to_iced(self.style.page_color())),
        );

        let width = bounds.width;
        let viewport_h = bounds.height as i32;
        let top = scroll_top(self.transcript, viewport_h);
        let range = self
            .transcript
            .visible_range(viewport_h, self.style.scaled(120.0));
        let sel_color = to_iced(self.style.selection_color());

        for row_index in range {
            if row_index >= self.transcript.rows().len() {
                continue;
            }
            let row = &self.transcript.rows()[row_index];
            let row_y = self.transcript.y_of_row(row_index) as f32 - top as f32 + bounds.y;
            let row_h = row_height_at(self.transcript, row_index) as f32;
            draw_row(
                renderer,
                defaults,
                self.transcript,
                row,
                row_index,
                row_y,
                row_h,
                width,
                bounds.x,
                self.style,
                self.theme_generation,
                self.member_realnames,
                state,
                sel_color,
                viewport,
            );
        }

        draw_scrollbar(renderer, bounds, self.transcript, viewport_h, self.style);
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::Text
    }
}

fn message_header_offset(row: &Row, style: &Style) -> f32 {
    if row.kind == RowKind::Message && !row.continues {
        style.scaled(18.0) as f32
    } else {
        0.0
    }
}

fn pointer_zone(
    row: &Row,
    row_y: f32,
    content_y: f32,
    x: f32,
    style: &Style,
) -> Option<PointerZone> {
    if row.kind != RowKind::Message {
        return Some(PointerZone::HeaderOrAvatar);
    }
    let local_y = content_y - row_y;
    let avatar_left = style.scaled(24.0) as f32;
    let avatar_top = style.scaled(9.0) as f32;
    let avatar = style.scaled(34.0) as f32;
    if x >= avatar_left
        && x <= avatar_left + avatar
        && local_y >= avatar_top
        && local_y <= avatar_top + avatar
    {
        return Some(PointerZone::HeaderOrAvatar);
    }
    if !row.continues && local_y < message_header_offset(row, style) {
        return Some(PointerZone::HeaderOrAvatar);
    }
    if local_y < style.scaled(4.0) as f32 {
        return Some(PointerZone::GapAbove);
    }
    None
}

fn pointer_in_body(row: &Row, row_y: f32, content_y: f32, x: f32, style: &Style) -> bool {
    pointer_zone(row, row_y, content_y, x, style).is_none()
}

fn draw_scrollbar<Renderer: renderer::Renderer>(
    renderer: &mut Renderer,
    bounds: Rectangle,
    transcript: &Transcript,
    viewport_h: i32,
    style: &Style,
) {
    let total: i32 = (0..transcript.rows().len())
        .map(|i| row_height_at(transcript, i))
        .sum();
    if total <= viewport_h {
        return;
    }
    let scroll_top = scroll_top(transcript, viewport_h) as f32;
    let track = style.scaled(4.0) as f32;
    let thumb_h =
        (bounds.height * bounds.height / total as f32).max(style.scaled(24.0) as f32);
    let max_top = (total - viewport_h).max(1) as f32;
    let thumb_y = bounds.y + (scroll_top / max_top) * (bounds.height - thumb_h);
    renderer.fill_quad(
        renderer::Quad {
            bounds: Rectangle::new(
                Point::new(bounds.x + bounds.width - track - 2.0, bounds.y),
                Size::new(track, bounds.height),
            ),
            ..Default::default()
        },
        iced::Background::Color(to_iced(style.divider_color())),
    );
    renderer.fill_quad(
        renderer::Quad {
            bounds: Rectangle::new(
                Point::new(bounds.x + bounds.width - track - 2.0, thumb_y),
                Size::new(track, thumb_h),
            ),
            ..Default::default()
        },
        iced::Background::Color(to_iced(style.muted_color())),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_row<Renderer: renderer::Renderer + text::Renderer<Font = iced::Font>>(
    renderer: &mut Renderer,
    defaults: &renderer::Style,
    transcript: &Transcript,
    row: &Row,
    row_index: usize,
    row_y: f32,
    row_h: f32,
    width: f32,
    origin_x: f32,
    style: &Style,
    theme_gen: u64,
    member_realnames: &std::collections::HashMap<String, String>,
    state: &WidgetState<Renderer::Paragraph>,
    sel_color: Color,
    viewport: &Rectangle,
) where
    Renderer::Paragraph: TextParagraph + Clone + Default,
{
    let row_bounds = Rectangle::new(Point::new(origin_x, row_y), Size::new(width, row_h));
    if !row_bounds.intersects(viewport) {
        return;
    }

    if row.kind == RowKind::DaySeparator || row.kind == RowKind::Event {
        let size = style.scaled(10.0) as f32;
        let p = paragraph_for_row(
            row,
            style,
            width - style.scaled(40.0) as f32,
            theme_gen,
            true,
            state,
            renderer,
            size,
            ParagraphOverlay::default(),
        );
        let h = p.min_bounds().height;
        let y = row_y + (row_h - h) / 2.0;
        draw_text(
            renderer,
            defaults,
            Rectangle::new(Point::new(origin_x, y), Size::new(width, h)),
            &p,
            iced::widget::text::Style {
                color: Some(to_iced(style.muted_color())),
            },
            viewport,
        );
        return;
    }

    if let Some(author) = &row.author {
        let avatar = style.scaled(34.0) as f32;
        let avatar_x = origin_x + style.scaled(24.0) as f32;
        let avatar_y = row_y + style.scaled(9.0) as f32;
        let fill = style.nick_avatar_fills()[style.nick_palette_index(author)];
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(Point::new(avatar_x, avatar_y), Size::new(avatar, avatar)),
                border: iced::Border {
                    radius: avatar.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            },
            iced::Background::Color(to_iced(fill)),
        );
        let initials = style.initials(author);
        let init_p = Renderer::Paragraph::with_text(text::Text {
            content: &initials,
            bounds: Size::new(avatar, avatar),
            size: Pixels(style.scaled(12.0) as f32),
            line_height: text::LineHeight::default(),
            font: bold(),
            align_x: text::Alignment::Center,
            align_y: Vertical::Center,
            shaping: text::Shaping::Basic,
            wrapping: text::Wrapping::None,
        });
        draw_text(
            renderer,
            defaults,
            Rectangle::new(Point::new(avatar_x, avatar_y), Size::new(avatar, avatar)),
            &init_p,
            iced::widget::text::Style {
                color: Some(to_iced(style.ink_color())),
            },
            viewport,
        );
    }

    let mut body_y = row_y + style.scaled(8.0) as f32;
    if !row.continues {
        let nick = row.author.as_deref().unwrap_or("");
        let nick_size = style.scaled(12.0) as f32;
        let real = member_realnames.get(nick).map(String::as_str).unwrap_or("");
        let suffix = if real.is_empty() {
            String::new()
        } else {
            format!(" · {real}")
        };
        let header_p = Renderer::Paragraph::with_spans(text::Text {
            content: &[
                Span::<(), iced::Font>::new(nick)
                    .color(to_iced(style.nick_color(nick)))
                    .size(Pixels(nick_size))
                    .font(bold()),
                Span::<(), iced::Font>::new(suffix)
                    .color(to_iced(style.muted_color()))
                    .size(Pixels(nick_size)),
            ],
            bounds: Size::new(body_width(width, style), nick_size * 2.0),
            size: Pixels(nick_size),
            line_height: text::LineHeight::default(),
            font: IA_WRITER,
            align_x: text::Alignment::Left,
            align_y: Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
        });
        draw_text(
            renderer,
            defaults,
            Rectangle::new(
                Point::new(origin_x + style.scaled(70.0) as f32, body_y),
                Size::new(body_width(width, style), nick_size),
            ),
            &header_p,
            iced::widget::text::Style::default(),
            viewport,
        );
        if let Some(time) = &row.time {
            let time_p = Renderer::Paragraph::with_text(text::Text {
                content: time.as_str(),
                bounds: Size::new(style.scaled(48.0) as f32, nick_size),
                size: Pixels(style.scaled(9.0) as f32),
                line_height: text::LineHeight::default(),
                font: IA_WRITER,
                align_x: text::Alignment::Right,
                align_y: Vertical::Top,
                shaping: text::Shaping::Basic,
                wrapping: text::Wrapping::None,
            });
            draw_text(
                renderer,
                defaults,
                Rectangle::new(
                    Point::new(
                        origin_x + width - style.scaled(82.0) as f32,
                        body_y,
                    ),
                    Size::new(style.scaled(48.0) as f32, nick_size),
                ),
                &time_p,
                iced::widget::text::Style {
                    color: Some(to_iced(style.muted_color())),
                },
                viewport,
            );
        }
        body_y += nick_size + style.scaled(4.0) as f32;
    }

    let body_w = body_width(width, style);
    let body_left = origin_x + style.scaled(70.0) as f32;
    let overlay = ParagraphOverlay {
        selection: transcript.range_for_row(row_index),
        hover_link: hovered_link_range(state, row_index, row),
        selection_color: sel_color,
    };
    let p = paragraph_for_row(
        row,
        style,
        body_w,
        theme_gen,
        false,
        state,
        renderer,
        style.scaled(13.0) as f32,
        overlay,
    );
    let body_bounds = Rectangle::new(
        Point::new(body_left, body_y),
        Size::new(body_w, (row_y + row_h - body_y).max(1.0)),
    );

    draw_text(
        renderer,
        defaults,
        body_bounds,
        &p,
        iced::widget::text::Style {
            color: Some(to_iced(style.ink_color())),
        },
        viewport,
    );
}

impl<'a, Message, Renderer> From<TranscriptWidget<'a>> for Element<'a, Message, Theme, Renderer>
where
    Message: From<TranscriptMessage> + Clone + 'static,
    Renderer: renderer::Renderer + text::Renderer<Font = iced::Font>,
    Renderer::Paragraph: TextParagraph<Font = iced::Font> + Clone + Default,
{
    fn from(widget: TranscriptWidget<'a>) -> Self {
        Element::new(widget)
    }
}
