//! The Connect sheet's live controls: one text field per profile field, a focus
//! handle per stop, and the scroll handles for the rail and the form. The shell
//! owns one for its whole life, so a field keeps its caret and undo between
//! opens, and a stop keeps its focus handle from frame to frame.

use std::collections::HashMap;

use gpui::{App, AppContext, Entity, FocusHandle, Focusable, ScrollHandle, Window};

use crate::model::connect::{ConnectField, Stop};
use crate::ui::input::SingleLineInput;

/// The key context of every Connect text field. The sheet's card owns Enter, Tab
/// and Space (`bind_connect_keys`), so the composer's keys never reach a field.
pub const FIELD_CONTEXT: &str = "ConnectField";

/// Every text field on the Connection tab, in Tab order.
const FIELDS: [ConnectField; 9] = [
    ConnectField::Name,
    ConnectField::Host,
    ConnectField::Port,
    ConnectField::Nick,
    ConnectField::Username,
    ConnectField::RealName,
    ConnectField::Autojoin,
    ConnectField::ServerPassword,
    ConnectField::NickServPassword,
];

pub struct ConnectControls {
    fields: HashMap<ConnectField, Entity<SingleLineInput>>,
    handles: HashMap<Stop, FocusHandle>,
    /// The rail's scroll, so a focused row can be brought into view.
    pub rail_scroll: ScrollHandle,
    /// The form's scroll, so a focused field can be brought into view.
    pub form_scroll: ScrollHandle,
}

impl ConnectControls {
    pub fn new(cx: &mut App) -> Self {
        let mut controls = Self {
            fields: HashMap::new(),
            handles: HashMap::new(),
            rail_scroll: ScrollHandle::new(),
            form_scroll: ScrollHandle::new(),
        };
        for field in FIELDS {
            let input = cx.new(|cx| {
                let input = SingleLineInput::new("", FIELD_CONTEXT, cx.focus_handle());
                if field.secret() {
                    input.masked()
                } else {
                    input
                }
            });
            // A field's stop is its input's own focus handle, so Tab lands in the caret.
            let handle = input.read(cx).focus_handle(cx);
            controls.handles.insert(Stop::Field(field), handle);
            controls.fields.insert(field, input);
        }
        controls
    }

    /// The text field for `field`.
    pub fn input(&self, field: ConnectField) -> &Entity<SingleLineInput> {
        &self.fields[&field]
    }

    /// Every field with its input, in Tab order. The shell watches these for edits.
    pub fn fields(&self) -> Vec<(ConnectField, Entity<SingleLineInput>)> {
        FIELDS
            .iter()
            .map(|field| (*field, self.fields[field].clone()))
            .collect()
    }

    /// Creates a focus handle for each stop that does not have one yet. Stops keep
    /// their handle for the window's life.
    pub fn ensure(&mut self, cx: &App, stops: &[Stop]) {
        for stop in stops {
            if !self.handles.contains_key(stop) {
                self.handles.insert(stop.clone(), cx.focus_handle());
            }
        }
    }

    /// The focus handle of `stop`, once `ensure` has made one.
    pub fn handle(&self, stop: &Stop) -> Option<&FocusHandle> {
        self.handles.get(stop)
    }

    /// The first of `stops` whose handle holds the window's focus, if any.
    pub fn focused(&self, stops: &[Stop], window: &Window) -> Option<Stop> {
        stops
            .iter()
            .find(|stop| {
                self.handle(stop)
                    .is_some_and(|handle| handle.is_focused(window))
            })
            .cloned()
    }
}
