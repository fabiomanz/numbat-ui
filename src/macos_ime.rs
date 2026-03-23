use eframe::egui;

fn decompose_mac_dead_key(c: char) -> Option<(char, char)> {
    match c {
        // Circumflex
        'â' => Some(('^', 'a')), 'ê' => Some(('^', 'e')), 'î' => Some(('^', 'i')),
        'ô' => Some(('^', 'o')), 'û' => Some(('^', 'u')),
        'Â' => Some(('^', 'A')), 'Ê' => Some(('^', 'E')), 'Î' => Some(('^', 'I')),
        'Ô' => Some(('^', 'O')), 'Û' => Some(('^', 'U')),
        // Grave
        'à' => Some(('`', 'a')), 'è' => Some(('`', 'e')), 'ì' => Some(('`', 'i')),
        'ò' => Some(('`', 'o')), 'ù' => Some(('`', 'u')),
        'À' => Some(('`', 'A')), 'È' => Some(('`', 'E')), 'Ì' => Some(('`', 'I')),
        'Ò' => Some(('`', 'O')), 'Ù' => Some(('`', 'U')),
        // Tilde
        'ã' => Some(('~', 'a')), 'õ' => Some(('~', 'o')), 'ñ' => Some(('~', 'n')),
        'Ã' => Some(('~', 'A')), 'Õ' => Some(('~', 'O')), 'Ñ' => Some(('~', 'N')),
        _ => None,
    }
}

/// Applies a workaround for macOS where mathematical dead keys (`^`, `~`, ``` ` ```)
/// otherwise swallow the next keystroke into an unwanted composition (like `ê`).
/// This reconstructs the individual keypresses cleanly.
pub fn fix_macos_dead_keys(raw_input: &mut egui::RawInput, last_dead_key: &mut Option<String>) {
    let mut new_events = Vec::new();

    for event in std::mem::take(&mut raw_input.events) {
        match event {
            egui::Event::Ime(egui::ImeEvent::Preedit(ref text)) => {
                // We only target programming symbols (Circumflex, Grave, Tilde) that cause annoyance in math expressions.
                // We leave natural language dead keys (like Acute) alone.
                if text == "^" || text == "`" || text == "~" {
                    *last_dead_key = Some(text.clone());
                    new_events.push(egui::Event::Ime(egui::ImeEvent::Commit(text.clone())));
                    new_events.push(egui::Event::Ime(egui::ImeEvent::Preedit(String::new())));
                } else {
                    new_events.push(event);
                }
            }
            egui::Event::Ime(egui::ImeEvent::Commit(ref text)) => {
                let mut uncombined = String::new();
                for c in text.chars() {
                    if let Some((dead, base)) = decompose_mac_dead_key(c) {
                        uncombined.push(dead);
                        uncombined.push(base);
                    } else {
                        uncombined.push(c);
                    }
                }

                if let Some(dead_key) = last_dead_key.take() {
                    if uncombined.starts_with(&dead_key) {
                        uncombined = uncombined[dead_key.len()..].to_string();
                    }
                }

                if !uncombined.is_empty() {
                    new_events.push(egui::Event::Ime(egui::ImeEvent::Commit(uncombined)));
                }
            }
            _ => {
                new_events.push(event);
            }
        }
    }
    
    raw_input.events = new_events;
}
