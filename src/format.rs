use eframe::egui;
use numbat::markup::{FormatType, FormattedString, Markup};

pub fn markup_to_layout_job(markup: &Markup) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for f_string in &markup.0 {
        append_markup(f_string, &mut job);
    }
    job
}

fn append_markup(f_string: &FormattedString, job: &mut egui::text::LayoutJob) {
    let FormattedString(_output_type, format_type, text) = f_string;
    
    let color = match format_type {
        FormatType::Whitespace => egui::Color32::WHITE,
        FormatType::Emphasized => egui::Color32::WHITE, // could be bold
        FormatType::Dimmed => egui::Color32::GRAY,
        FormatType::Text => egui::Color32::WHITE,
        FormatType::String => egui::Color32::GREEN,
        FormatType::Keyword => egui::Color32::from_rgb(255, 100, 255), // Magenta
        FormatType::Value => egui::Color32::YELLOW,
        FormatType::Unit => egui::Color32::from_rgb(0, 255, 255), // Cyan
        FormatType::Identifier => egui::Color32::WHITE,
        FormatType::TypeIdentifier => egui::Color32::from_rgb(100, 150, 255), // Blue
        FormatType::Operator => egui::Color32::WHITE,
        FormatType::Decorator => egui::Color32::GREEN,
    };

    let format = egui::TextFormat {
        font_id: egui::FontId::new(14.0, egui::FontFamily::Monospace),
        color,
        ..Default::default()
    };

    if matches!(format_type, FormatType::TypeIdentifier) {
        // Italic
    }

    job.append(text, 0.0, format);
}
