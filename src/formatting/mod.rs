// Format currency
pub fn format_currency(value: f64) -> String {
    format!("${:.2}", value)
}

// Format number with thousands separator
pub fn format_number_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in s.chars().rev() {
        if count == 3 {
            result.push(',');
            count = 0;
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}

// Format remaining time
pub fn format_remaining_time(minutes: u64) -> String {
    if minutes == 0 {
        "Block expired".to_string()
    } else if minutes < 60 {
        format!("{}m left", minutes)
    } else {
        let hours = minutes / 60;
        let mins = minutes % 60;
        if mins > 0 {
            format!("{}h {}m left", hours, mins)
        } else {
            format!("{}h left", hours)
        }
    }
}
