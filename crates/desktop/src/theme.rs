//! The visual design of the app: color palettes (derived from the numbat
//! icon's periwinkle/salmon colors), egui visuals, and rendering of numbat
//! markup into styled text.

use egui::text::LayoutJob;
use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextFormat};
use numbat::markup::{FormatType, FormattedString, Markup};

use crate::config::ThemeChoice;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub dark: bool,

    /// Window background.
    pub bg: Color32,
    /// Slightly raised surfaces: top bar, input bar.
    pub bg_raised: Color32,
    /// History entry cards.
    pub card: Color32,
    /// Borders and separators.
    pub border: Color32,

    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,

    /// Brand accent (periwinkle, from the app icon).
    pub accent: Color32,
    /// Errors (salmon, from the app icon).
    pub error: Color32,

    // Syntax colors for numbat markup.
    pub value: Color32,
    pub unit: Color32,
    pub keyword: Color32,
    pub string: Color32,
    pub type_id: Color32,
    pub operator: Color32,
    pub decorator: Color32,
}

pub const DARK: Palette = Palette {
    dark: true,
    bg: Color32::from_rgb(0x11, 0x13, 0x18),
    bg_raised: Color32::from_rgb(0x16, 0x19, 0x20),
    card: Color32::from_rgb(0x1a, 0x1e, 0x27),
    border: Color32::from_rgb(0x2a, 0x2f, 0x3a),
    text: Color32::from_rgb(0xe8, 0xea, 0xf0),
    text_dim: Color32::from_rgb(0x9a, 0xa1, 0xb0),
    text_faint: Color32::from_rgb(0x62, 0x68, 0x76),
    accent: Color32::from_rgb(0x8f, 0x9a, 0xf0),
    error: Color32::from_rgb(0xef, 0x8e, 0x96),
    value: Color32::from_rgb(0xf0, 0xc6, 0x74),
    unit: Color32::from_rgb(0x74, 0xd4, 0xc2),
    keyword: Color32::from_rgb(0xc7, 0x92, 0xea),
    string: Color32::from_rgb(0xa3, 0xd7, 0x8f),
    type_id: Color32::from_rgb(0x84, 0xab, 0xf5),
    operator: Color32::from_rgb(0xb4, 0xba, 0xc8),
    decorator: Color32::from_rgb(0xa3, 0xd7, 0x8f),
};

pub const LIGHT: Palette = Palette {
    dark: false,
    bg: Color32::from_rgb(0xf4, 0xf5, 0xf8),
    bg_raised: Color32::from_rgb(0xec, 0xee, 0xf3),
    card: Color32::WHITE,
    border: Color32::from_rgb(0xdd, 0xe0, 0xe8),
    text: Color32::from_rgb(0x22, 0x25, 0x2d),
    text_dim: Color32::from_rgb(0x6e, 0x74, 0x84),
    text_faint: Color32::from_rgb(0xa2, 0xa8, 0xb6),
    accent: Color32::from_rgb(0x59, 0x67, 0xd6),
    error: Color32::from_rgb(0xcf, 0x4a, 0x56),
    value: Color32::from_rgb(0xa5, 0x76, 0x0e),
    unit: Color32::from_rgb(0x0e, 0x85, 0x74),
    keyword: Color32::from_rgb(0x8a, 0x46, 0xbd),
    string: Color32::from_rgb(0x46, 0x86, 0x30),
    type_id: Color32::from_rgb(0x34, 0x5c, 0xc0),
    operator: Color32::from_rgb(0x50, 0x56, 0x64),
    decorator: Color32::from_rgb(0x46, 0x86, 0x30),
};

/// Resolves the configured theme against the system preference.
pub fn palette_for(choice: ThemeChoice, ctx: &egui::Context) -> Palette {
    match choice {
        ThemeChoice::Dark => DARK,
        ThemeChoice::Light => LIGHT,
        ThemeChoice::System => match ctx.input(|i| i.raw.system_theme) {
            Some(egui::Theme::Light) => LIGHT,
            _ => DARK,
        },
    }
}

/// Applies the palette to egui's global style.
pub fn apply(ctx: &egui::Context, palette: &Palette, font_size: f32) {
    let mut visuals = if palette.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.bg;
    visuals.extreme_bg_color = palette.bg_raised;
    visuals.faint_bg_color = palette.bg_raised;

    visuals.override_text_color = Some(palette.text);
    visuals.hyperlink_color = palette.accent;
    visuals.selection.bg_fill = palette.accent.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.error_fg_color = palette.error;
    visuals.warn_fg_color = palette.value;

    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.window_stroke = Stroke::new(1.0, palette.border);

    for (widget, fill) in [
        (&mut visuals.widgets.noninteractive, palette.card),
        (&mut visuals.widgets.inactive, palette.card),
        (&mut visuals.widgets.hovered, palette.border),
        (&mut visuals.widgets.active, palette.border),
        (&mut visuals.widgets.open, palette.card),
    ] {
        widget.corner_radius = CornerRadius::same(7);
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent.gamma_multiply(0.7));
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text_dim);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, palette.text);

    ctx.set_visuals(visuals);

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        FontId::new(font_size, FontFamily::Monospace),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(font_size, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(font_size, FontFamily::Proportional),
    );
    ctx.set_global_style(style);
}

impl Palette {
    pub fn markup_color(&self, format_type: &FormatType) -> Color32 {
        match format_type {
            FormatType::Whitespace | FormatType::Text | FormatType::Identifier => self.text,
            FormatType::Emphasized => self.text,
            FormatType::Dimmed => self.text_dim,
            FormatType::String => self.string,
            FormatType::Keyword => self.keyword,
            FormatType::Value => self.value,
            FormatType::Unit => self.unit,
            FormatType::TypeIdentifier => self.type_id,
            FormatType::Operator => self.operator,
            FormatType::Decorator => self.decorator,
        }
    }
}

/// Renders numbat markup as a styled egui `LayoutJob`.
pub fn markup_job(markup: &Markup, palette: &Palette, font_size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for FormattedString(_, format_type, text) in &markup.0 {
        let format = TextFormat {
            font_id: FontId::new(font_size, FontFamily::Monospace),
            color: palette.markup_color(format_type),
            italics: matches!(format_type, FormatType::TypeIdentifier),
            ..Default::default()
        };
        job.append(text.as_ref(), 0.0, format);
    }
    // Markup from numbat often ends with a trailing newline; drop it so
    // labels don't get an empty last line.
    while job.text.ends_with('\n') {
        let len = job.text.len() - 1;
        job.sections.retain_mut(|section| {
            section.byte_range.end = section.byte_range.end.min(len);
            section.byte_range.start < section.byte_range.end
        });
        job.text.truncate(len);
    }
    job
}

/// Syntax highlighting for the *input* line while typing (numbat markup is
/// only available after evaluation, so this is a small standalone lexer).
pub fn highlight_input(text: &str, palette: &Palette, font_size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font_id = FontId::new(font_size, FontFamily::Monospace);
    let mut push = |slice: &str, color: Color32| {
        if !slice.is_empty() {
            job.append(
                slice,
                0.0,
                TextFormat {
                    font_id: font_id.clone(),
                    color,
                    ..Default::default()
                },
            );
        }
    };

    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &text[i..];
        let c = rest.chars().next().unwrap();

        if c == '#' {
            push(rest, palette.text_faint);
            break;
        } else if c == '"' {
            let end = rest[1..].find('"').map(|p| i + p + 2).unwrap_or(text.len());
            push(&text[i..end], palette.string);
            i = end;
        } else if c.is_ascii_digit()
            || (c == '.' && rest[1..].starts_with(|d: char| d.is_ascii_digit()))
        {
            let end = i + rest
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '_'))
                .unwrap_or(rest.len());
            push(&text[i..end], palette.value);
            i = end;
        } else if c.is_alphabetic() || c == '_' {
            let end = i + rest
                .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                .unwrap_or(rest.len());
            let word = &text[i..end];
            let color = if numbat::keywords::KEYWORDS.contains(&word) {
                palette.keyword
            } else {
                palette.text
            };
            push(word, color);
            i = end;
        } else if "+-*/^=<>!·×÷²³→➞".contains(c) {
            push(&text[i..i + c.len_utf8()], palette.operator);
            i += c.len_utf8();
        } else {
            push(&text[i..i + c.len_utf8()], palette.text);
            i += c.len_utf8();
        }
    }

    job
}
