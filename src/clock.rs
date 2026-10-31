use time::OffsetDateTime;

/// Format the current local time for the header.
///
/// Local time is used when the system time zone can be determined; otherwise
/// the formatter falls back to UTC rather than hiding the clock.
pub fn now(format: &str) -> String {
    let datetime = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());

    format_datetime(datetime, format)
}

/// Format `datetime` using a small strftime-compatible subset.
///
/// Unknown specifiers are left untouched, so a typo in the configuration is
/// visible in the header instead of silently disappearing.
pub fn format_datetime(datetime: OffsetDateTime, format: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const WEEKDAYS: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];

    let mut output = String::with_capacity(format.len() + 8);
    let mut chars = format.chars();

    while let Some(character) = chars.next() {
        if character != '%' {
            output.push(character);
            continue;
        }

        let Some(specifier) = chars.next() else {
            output.push('%');
            break;
        };

        match specifier {
            '%' => output.push('%'),
            'Y' => output.push_str(&format!("{:04}", datetime.year())),
            'y' => output.push_str(&format!("{:02}", datetime.year().rem_euclid(100))),
            'm' => output.push_str(&format!("{:02}", datetime.month() as u8)),
            'd' => output.push_str(&format!("{:02}", datetime.day())),
            'e' => output.push_str(&format!("{:2}", datetime.day())),
            'H' => output.push_str(&format!("{:02}", datetime.hour())),
            'I' => {
                let hour = match datetime.hour() % 12 {
                    0 => 12,
                    hour => hour,
                };
                output.push_str(&format!("{hour:02}"));
            }
            'M' => output.push_str(&format!("{:02}", datetime.minute())),
            'S' => output.push_str(&format!("{:02}", datetime.second())),
            'p' => output.push_str(if datetime.hour() < 12 { "AM" } else { "PM" }),
            'a' => output.push_str(&WEEKDAYS[weekday_index(datetime)][..3]),
            'A' => output.push_str(WEEKDAYS[weekday_index(datetime)]),
            'b' => output.push_str(&MONTHS[datetime.month() as usize - 1][..3]),
            'B' => output.push_str(MONTHS[datetime.month() as usize - 1]),
            'j' => output.push_str(&format!("{:03}", datetime.ordinal())),
            'F' => output.push_str(&format_datetime(datetime, "%Y-%m-%d")),
            'T' => output.push_str(&format_datetime(datetime, "%H:%M:%S")),
            'R' => output.push_str(&format_datetime(datetime, "%H:%M")),
            'z' => output.push_str(&offset_label(datetime)),
            other => {
                output.push('%');
                output.push(other);
            }
        }
    }

    output
}

fn weekday_index(datetime: OffsetDateTime) -> usize {
    datetime.weekday().number_from_sunday() as usize - 1
}

fn offset_label(datetime: OffsetDateTime) -> String {
    let offset = datetime.offset();
    let seconds = i32::from(offset.whole_hours()) * 3600
        + i32::from(offset.minutes_past_hour()) * 60
        + i32::from(offset.seconds_past_minute());
    let sign = if seconds < 0 { '-' } else { '+' };
    let seconds = seconds.abs();
    let hours = seconds / 3600;
    let minutes = seconds % 3600 / 60;

    format!("{sign}{hours:02}{minutes:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::UtcOffset;

    fn sample() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000)
            .expect("timestamp")
            .to_offset(UtcOffset::from_hms(9, 0, 0).expect("offset"))
    }

    #[test]
    fn formats_common_date_and_time_specifiers() {
        assert_eq!(
            format_datetime(sample(), "%Y-%m-%d %H:%M:%S"),
            "2023-11-15 07:13:20"
        );
        assert_eq!(format_datetime(sample(), "%F %T"), "2023-11-15 07:13:20");
        assert_eq!(format_datetime(sample(), "%R"), "07:13");
    }

    #[test]
    fn formats_names_and_twelve_hour_clock() {
        assert_eq!(
            format_datetime(sample(), "%a %A %b %B %I:%M %p"),
            "Wed Wednesday Nov November 07:13 AM"
        );
    }

    #[test]
    fn formats_midnight_as_twelve_am() {
        let midnight = OffsetDateTime::from_unix_timestamp(0).expect("timestamp");

        assert_eq!(format_datetime(midnight, "%I:%M %p"), "12:00 AM");
        assert_eq!(format_datetime(midnight, "%j"), "001");
    }

    #[test]
    fn formats_the_utc_offset() {
        assert_eq!(format_datetime(sample(), "%z"), "+0900");
        assert_eq!(
            format_datetime(
                sample().to_offset(UtcOffset::from_hms(-3, -30, 0).expect("offset")),
                "%z"
            ),
            "-0330"
        );
    }

    #[test]
    fn keeps_literal_percent_unknown_specifiers_and_trailing_percent() {
        assert_eq!(format_datetime(sample(), "100%% %Q"), "100% %Q");
        assert_eq!(format_datetime(sample(), "done %"), "done %");
    }
}
