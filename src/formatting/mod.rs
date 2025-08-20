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
pub fn format_remaining_time(minutes: i64) -> String {
    if minutes < 60 {
        format!("{}m left", minutes)
    } else {
        let hours = minutes / 60;
        let mins = minutes % 60;
        if hours > 0 {
            format!("{}h {}m left", hours, mins)
        } else {
            format!("{}h left", hours)
        }
    }
}
