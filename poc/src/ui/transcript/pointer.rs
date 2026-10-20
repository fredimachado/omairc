//! Hit testing for the transcript: which row and character sits under a
//! pointer. Pure over the bounds and layouts the last prepaint recorded.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{Bounds, Pixels, Point};

use crate::model::links::{conversation_channel_hits, invite_hit};
use crate::model::{map_pointer, PointerZone, Pos, Row};
use crate::ui::message_row::RowLayoutMap;
use crate::util::scaled_px;
use crate::model::Style;

/// Window-space bounds of each rendered row, recorded at prepaint.
pub type RowBounds = Rc<RefCell<HashMap<usize, Bounds<Pixels>>>>;

pub fn row_at(bounds: &RowBounds, position: Point<Pixels>) -> Option<usize> {
    bounds
        .borrow()
        .iter()
        .filter(|(_, b)| b.contains(&position))
        .map(|(ix, _)| *ix)
        .min()
}

pub fn first_visible(bounds: &RowBounds, viewport: Bounds<Pixels>) -> Option<usize> {
    bounds
        .borrow()
        .iter()
        .filter(|(_, b)| b.intersects(&viewport))
        .map(|(ix, _)| *ix)
        .min()
}

pub fn last_visible(bounds: &RowBounds, viewport: Bounds<Pixels>) -> Option<usize> {
    bounds
        .borrow()
        .iter()
        .filter(|(_, b)| b.intersects(&viewport))
        .map(|(ix, _)| *ix)
        .max()
}

/// The text position under `position` in row `row_ix`. Gutters above the body
/// snap to the row's header or gap zone.
pub fn pos_at(
    rows: &[Row],
    layouts: &RowLayoutMap,
    bounds: &RowBounds,
    position: Point<Pixels>,
    row_ix: usize,
    app_style: &Style,
) -> Option<Pos> {
    let layouts = layouts.borrow();
    let layout = layouts.get(&row_ix)?;
    let row_bounds = bounds.borrow().get(&row_ix).cloned()?;
    let local_y = position.y - row_bounds.origin.y;
    if local_y < layout.avatar_top + layout.header_height {
        return Some(map_pointer(rows, row_ix, PointerZone::HeaderOrAvatar));
    }
    if local_y < (layout.body_bounds.origin.y - row_bounds.origin.y).max(scaled_px(app_style, 0.))
    {
        return Some(map_pointer(rows, row_ix, PointerZone::GapAbove));
    }
    match layout.layout.index_for_position(position) {
        Ok(offset) | Err(offset) => Some(Pos { row: row_ix, offset }),
    }
}

/// The link under `position`, as its row and byte range in that row's body. A
/// link is an http(s) URL in the body, the channel of a Status INVITE line, or
/// a channel name in a chat line (`chantypes` is the network's CHANTYPES).
pub fn link_at(
    rows: &[Row],
    layouts: &RowLayoutMap,
    bounds: &RowBounds,
    chantypes: &str,
    position: Point<Pixels>,
) -> Option<(usize, Range<usize>)> {
    let row_ix = row_at(bounds, position)?;
    let row = rows.get(row_ix)?;
    let layout = layouts.borrow().get(&row_ix)?.layout.clone();
    let offset = layout.index_for_position(position).ok()?;
    let invite = invite_hit(row).map(|(_, range)| range);
    let channels = conversation_channel_hits(row, chantypes)
        .into_iter()
        .map(|span| span.range);
    row.body
        .links
        .iter()
        .cloned()
        .chain(invite)
        .chain(channels)
        .find(|range| range.contains(&offset))
        .map(|range| (row_ix, range))
}
