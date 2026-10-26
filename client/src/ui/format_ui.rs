use gpui::{TextRun, TextStyle, UnderlineStyle, px};
use crate::model::{Style, StyleRun, TextStyle as IrcStyle};

use crate::util::{to_gpui, FONT_FAMILY};

/// Whether mIRC colors paint (`/pref colors`). Off, runs keep their formatting and take the theme ink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrcColors {
    Shown,
    Stripped,
}

impl IrcColors {
    pub fn from_pref(on: bool) -> Self {
        if on {
            Self::Shown
        } else {
            Self::Stripped
        }
    }
}

impl Default for IrcColors {
    fn default() -> Self {
        Self::Shown
    }
}

pub fn gpui_text_style(style: &Style, size_px: f32, bold: bool) -> TextStyle {
    let mut ts = TextStyle {
        font_family: FONT_FAMILY.into(),
        font_size: crate::util::scaled_px(style, size_px).into(),
        ..Default::default()
    };
    if bold {
        ts.font_weight = gpui::FontWeight::BOLD;
    }
    ts.color = to_gpui(style.ink_color());
    ts
}

pub fn irc_runs_to_gpui(
    plain: &str,
    runs: &[StyleRun],
    style: &Style,
    base: &TextStyle,
    colors: IrcColors,
    selection: Option<std::ops::Range<usize>>,
) -> Vec<TextRun> {
    let mut gpui_runs = Vec::new();
    let mut ix = 0;
    for run in runs {
        if ix >= run.range.start {
            let start = run.range.start.max(ix);
            if start > ix {
                gpui_runs.push(base.clone().to_run(start - ix));
                ix = start;
            }
            if ix >= run.range.end {
                continue;
            }
            let len = run.range.end - ix;
            gpui_runs.push(irc_run_to_gpui(&run.style, style, base, colors, len));
            ix = run.range.end;
        }
    }
    if ix < plain.len() {
        gpui_runs.push(base.clone().to_run(plain.len() - ix));
    }
    if gpui_runs.is_empty() {
        gpui_runs.push(base.clone().to_run(plain.len()));
    }

    if let Some(sel) = selection {
        return apply_selection_runs(plain, gpui_runs, sel, style);
    }
    gpui_runs
}

fn irc_run_to_gpui(
    irc: &IrcStyle,
    style: &Style,
    base: &TextStyle,
    colors: IrcColors,
    len: usize,
) -> TextRun {
    let mut ts = base.clone();
    if irc.bold {
        ts.font_weight = gpui::FontWeight::BOLD;
    }
    if irc.italic {
        ts.font_style = gpui::FontStyle::Italic;
    }
    if irc.strikethrough {
        ts.strikethrough = Some(gpui::StrikethroughStyle {
            color: None,
            thickness: px(1.),
        });
    }
    if irc.underline {
        ts.underline = Some(UnderlineStyle {
            color: None,
            thickness: px(1.),
            wavy: false,
        });
    }
    if colors == IrcColors::Shown {
        if let Some(fg) = irc.fg {
            ts.color = to_gpui(fg);
        }
        if let Some(bg) = irc.bg {
            ts.background_color = Some(to_gpui(bg));
        }
    }
    if irc.reverse {
        let fg = ts.color;
        ts.color = to_gpui(style.page_color());
        ts.background_color = Some(fg);
    }
    ts.to_run(len)
}

fn apply_selection_runs(
    plain: &str,
    runs: Vec<TextRun>,
    selection: std::ops::Range<usize>,
    style: &Style,
) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut offset = 0;
    for run in runs {
        let run_end = offset + run.len;
        let run_range = offset..run_end;
        offset = run_end;
        if run_end <= selection.start || run_range.start >= selection.end {
            out.push(run);
            continue;
        }
        let sel_start = selection.start.max(run_range.start);
        let sel_end = selection.end.min(run_end);
        if run_range.start < sel_start {
            let mut before = run.clone();
            before.len = sel_start - run_range.start;
            out.push(before);
        }
        let mut selected = run.clone();
        selected.len = sel_end - sel_start;
        selected.color = to_gpui(crate::model::Color::rgb(1., 1., 1.));
        selected.background_color = Some(to_gpui(style.selection_color()));
        out.push(selected);
        if sel_end < run_end {
            let mut after = run;
            after.len = run_end - sel_end;
            out.push(after);
        }
    }
    if out.is_empty() {
        out.push(
            gpui_text_style(style, 13., false)
                .to_run(plain.len()),
        );
    }
    out
}

pub fn link_underline_run(
    runs: Vec<TextRun>,
    link_range: std::ops::Range<usize>,
    underline: bool,
) -> Vec<TextRun> {
    if !underline {
        return runs;
    }
    let mut out = Vec::new();
    let mut offset = 0;
    for run in runs {
        let run_end = offset + run.len;
        let run_range = offset..run_end;
        offset = run_end;
        if run_end <= link_range.start || run_range.start >= link_range.end {
            out.push(run);
            continue;
        }
        let u_start = link_range.start.max(run_range.start);
        let u_end = link_range.end.min(run_end);
        if run_range.start < u_start {
            let mut before = run.clone();
            before.len = u_start - run_range.start;
            out.push(before);
        }
        let mut linked = run.clone();
        linked.len = u_end - u_start;
        linked.underline = Some(UnderlineStyle {
            color: None,
            thickness: px(1.),
            wavy: false,
        });
        out.push(linked);
        if u_end < run_end {
            let mut after = run;
            after.len = run_end - u_end;
            out.push(after);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::format::format_irc;
    use crate::model::theme::Theme;
    use crate::model::{Color, Style};
    use crate::util::to_gpui;

    #[test]
    fn stripped_keeps_bold_and_theme_ink_shown_paints_mirc_red() {
        let style = Style::new(Theme::defaults_dark(), 1.0);
        let formatted = format_irc("\x02\x0304bold red");
        let base = gpui_text_style(&style, 13., false);

        let stripped = irc_runs_to_gpui(
            &formatted.plain,
            &formatted.runs,
            &style,
            &base,
            IrcColors::Stripped,
            None,
        );
        let stripped_run = stripped
            .iter()
            .find(|run| run.len == formatted.plain.len())
            .expect("one run covers the plain text");
        assert_eq!(stripped_run.color, base.color);
        assert_eq!(stripped_run.font.weight, gpui::FontWeight::BOLD);

        let shown = irc_runs_to_gpui(
            &formatted.plain,
            &formatted.runs,
            &style,
            &base,
            IrcColors::Shown,
            None,
        );
        let shown_run = shown
            .iter()
            .find(|run| run.len == formatted.plain.len())
            .expect("one run covers the plain text");
        let red = Color::from_hex("#FF0000").expect("palette red");
        assert_eq!(shown_run.color, to_gpui(red));
        assert_eq!(shown_run.font.weight, gpui::FontWeight::BOLD);
    }
}
