// Text editing follows gpui 0.2.2 examples/input.rs (Copyright Zed Industries, Apache-2.0).
// Adapted for wrapped multiline composition and the platform UTF-16 input protocol.
use std::ops::Range;

use gpui::{prelude::*, *};
use unicode_segmentation::*;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Submit,
        Newline,
        Up,
        Down,
    ]
);

pub struct TextInput {
    pub focus_handle: FocusHandle,
    pub content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Vec<(usize, WrappedLine, Bounds<Pixels>)>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    pub compact: bool,
}

impl TextInput {
    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        _window.focus(&self.focus_handle);
        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        for (offset, line, bounds) in &self.last_layout {
            if position.y < bounds.bottom() {
                let index = line
                    .closest_index_for_position(position - bounds.origin, px(22.))
                    .unwrap_or_else(|index| index);
                return (offset + index).min(self.content.len());
            }
        }
        self.content.len()
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|selection| {
                let start = utf16_offset(new_text, selection.start);
                let end = utf16_offset(new_text, selection.end);
                range.start + start..range.start + end
            })
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let point = self.position_for_offset(range.start)?;
        let _ = bounds;
        Some(Bounds::new(point, size(px(2.), px(22.))))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_mouse_position(point)))
    }
}

fn utf16_offset(text: &str, offset: usize) -> usize {
    let mut count = 0;
    text.char_indices()
        .find_map(|(index, ch)| {
            if count >= offset {
                Some(index)
            } else {
                count += ch.len_utf16();
                None
            }
        })
        .unwrap_or(text.len())
}

pub struct Submitted;
impl EventEmitter<Submitted> for TextInput {}

impl TextInput {
    pub fn new(placeholder: &str, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.to_owned().into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: Vec::new(),
            last_bounds: None,
            is_selecting: false,
            compact: false,
        }
    }
    pub fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self.marked_range = None;
        cx.notify();
    }
    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        if self.marked_range.is_none() {
            cx.emit(Submitted);
        }
    }
    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if !self.compact {
            self.replace_text_in_range(None, "\n", window, cx);
        }
    }
    fn position_for_offset(&self, offset: usize) -> Option<Point<Pixels>> {
        self.last_layout.iter().find_map(|(start, line, bounds)| {
            if offset >= *start && offset <= start + line.len() {
                line.position_for_index(offset - start, px(22.))
                    .map(|p| p + bounds.origin)
            } else {
                None
            }
        })
    }
    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.position_for_offset(self.cursor_offset()) {
            self.move_to(
                self.index_for_mouse_position(p - point(px(0.), px(22.))),
                cx,
            );
        }
    }
    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.position_for_offset(self.cursor_offset()) {
            self.move_to(
                self.index_for_mouse_position(p + point(px(0.), px(22.))),
                cx,
            );
        }
    }
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("up", Up, Some("TextInput")),
        KeyBinding::new("down", Down, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("TextInput")),
        KeyBinding::new("cmd-v", Paste, Some("TextInput")),
        KeyBinding::new("cmd-c", Copy, Some("TextInput")),
        KeyBinding::new("cmd-x", Cut, Some("TextInput")),
        KeyBinding::new("home", Home, Some("TextInput")),
        KeyBinding::new("end", End, Some("TextInput")),
        KeyBinding::new("cmd-left", Home, Some("TextInput")),
        KeyBinding::new("cmd-right", End, Some("TextInput")),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some("TextInput")),
        KeyBinding::new("enter", Submit, Some("TextInput")),
        KeyBinding::new("shift-enter", Newline, Some("TextInput")),
    ]);
}

struct TextElement {
    input: Entity<TextInput>,
}
impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = Vec<(usize, WrappedLine, Bounds<Pixels>)>;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let input = self.input.read(cx);
        let rows = input
            .last_layout
            .iter()
            .map(|(_, line, _)| line.size(px(22.)).height)
            .fold(px(0.), |a, b| a + b);
        style.size.height = rows.max(px(if input.compact { 22. } else { 44. })).into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let text = if input.content.is_empty() {
            input.placeholder.clone()
        } else {
            input.content.clone()
        };
        let style = window.text_style();
        let color = if input.content.is_empty() {
            rgb(
                if matches!(
                    window.appearance(),
                    WindowAppearance::Light | WindowAppearance::VibrantLight
                ) {
                    0x6d6d78
                } else {
                    0x9595a1
                },
            )
            .into()
        } else {
            style.color
        };
        let base = TextRun {
            len: text.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let mut runs = Vec::new();
        if let Some(marked) = &input.marked_range {
            if marked.start > 0 {
                runs.push(TextRun {
                    len: marked.start,
                    ..base.clone()
                });
            }
            runs.push(TextRun {
                len: marked.len(),
                underline: Some(UnderlineStyle {
                    color: Some(color),
                    thickness: px(1.),
                    wavy: false,
                }),
                ..base.clone()
            });
            if marked.end < text.len() {
                runs.push(TextRun {
                    len: text.len() - marked.end,
                    ..base
                });
            }
        } else {
            runs.push(base);
        }
        let lines = window
            .text_system()
            .shape_text(text, px(14.), &runs, Some(bounds.size.width), None)
            .unwrap_or_default();
        let mut y = bounds.top();
        let mut offset = 0;
        lines
            .into_iter()
            .map(|line| {
                let height = line.size(px(22.)).height;
                let result = (
                    offset,
                    line.clone(),
                    Bounds::new(point(bounds.left(), y), size(bounds.size.width, height)),
                );
                offset += line.len() + 1;
                y += height;
                result
            })
            .collect()
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        lines: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let input = self.input.read(cx);
        let focus = input.focus_handle.clone();
        let selection = input.selected_range.clone();
        let cursor = input.cursor_offset();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for (start, line, area) in lines.iter() {
            if !selection.is_empty() {
                // Paint selection by grapheme so wrapping and newline boundaries remain correct.
                for (index, grapheme) in line.text.grapheme_indices(true) {
                    let absolute = start + index;
                    if absolute < selection.end && absolute + grapheme.len() > selection.start {
                        if let Some(p) = line.position_for_index(index, px(22.)) {
                            let next = line
                                .position_for_index(index + grapheme.len(), px(22.))
                                .unwrap_or(p + point(px(8.), px(0.)));
                            let width = if next.y == p.y {
                                (next.x - p.x).max(px(2.))
                            } else {
                                (area.size.width - p.x).max(px(2.))
                            };
                            window.paint_quad(fill(
                                Bounds::new(area.origin + p, size(width, px(22.))),
                                rgba(0x7777dd55),
                            ));
                        }
                    }
                }
            }
            let _ = line.paint(
                area.origin,
                px(22.),
                TextAlign::Left,
                Some(*area),
                window,
                cx,
            );
            if selection.is_empty()
                && focus.is_focused(window)
                && cursor >= *start
                && cursor <= start + line.len()
            {
                if let Some(p) = line.position_for_index(cursor - start, px(22.)) {
                    window.paint_quad(fill(
                        Bounds::new(area.origin + p, size(px(1.5), px(22.))),
                        rgb(
                            if matches!(
                                window.appearance(),
                                WindowAppearance::Light | WindowAppearance::VibrantLight
                            ) {
                                0x6b50a2
                            } else {
                                0xc0baff
                            },
                        ),
                    ));
                }
            }
        }
        self.input.update(cx, |input, cx| {
            let old_height = input
                .last_layout
                .iter()
                .map(|(_, _, b)| b.size.height)
                .fold(px(0.), |a, b| a + b);
            let new_height = lines
                .iter()
                .map(|(_, _, b)| b.size.height)
                .fold(px(0.), |a, b| a + b);
            input.last_layout = lines.clone();
            input.last_bounds = Some(bounds);
            if old_height != new_height {
                cx.notify();
            }
        });
    }
}
impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("native-text-input")
            .w_full()
            .key_context("TextInput")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::newline))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .text_color(rgb(
                if matches!(
                    window.appearance(),
                    WindowAppearance::Light | WindowAppearance::VibrantLight
                ) {
                    0x202126
                } else {
                    0xe9e9ef
                },
            ))
            .text_size(px(14.))
            .line_height(px(22.))
            .child(TextElement { input: cx.entity() })
    }
}
impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[gpui::test]
    fn unicode_selection_and_ime_do_not_submit_composition(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, cx| TextInput::new("", cx));
        let submissions = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = submissions.clone();
        let subscription = window
            .update(cx, |_, _, cx| {
                cx.subscribe(&cx.entity(), move |_, _, _: &Submitted, _| {
                    count.set(count.get() + 1)
                })
            })
            .unwrap();
        window
            .update(cx, |editor, window, cx| {
                editor.set_text("a🙂e\u{301}".into(), cx);
                assert_eq!(editor.range_from_utf16(&(1..3)), 1..5);
                assert_eq!(editor.previous_boundary(editor.content.len()), 5);
                editor.replace_and_mark_text_in_range(Some(1..3), "日本", Some(1..2), window, cx);
                assert_eq!(editor.content.as_ref(), "a日本e\u{301}");
                assert_eq!(editor.marked_text_range(window, cx), Some(1..3));
                assert_eq!(
                    editor.selected_text_range(false, window, cx).unwrap().range,
                    2..3
                );
                editor.submit(&Submit, window, cx);
            })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(submissions.get(), 0);
        window
            .update(cx, |editor, window, cx| {
                editor.unmark_text(window, cx);
                editor.submit(&Submit, window, cx);
                editor.selected_range = 1..7;
                editor.replace_text_in_range(None, "🙂", window, cx);
                assert_eq!(editor.content.as_ref(), "a🙂e\u{301}");
            })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(submissions.get(), 1);
        drop(subscription);
    }
}
