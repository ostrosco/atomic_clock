pub fn to_timestamp(year: u16, doy: u16, hour: u16, minute: u16) -> u32 {
    let year_u32: u32 = year.into();
    let doy_u32: u32 = doy.into();
    let hour_u32: u32 = hour.into();
    let minute_u32: u32 = minute.into();

    let secs_in_day = 86400;
    let days_in_year = 365;
    let num_leaps = calc_num_leap_years(year);
    ((year_u32 - 1970) * days_in_year + doy_u32 - 1) * secs_in_day
        + hour_u32 * 60 * 60
        + minute_u32 * 60
        + num_leaps * secs_in_day
}

pub fn from_timestamp(unix_ts: u32) -> (u32, u32, u32, u32, u32) {
    let mut unix_ts = unix_ts;
    let year = unix_ts / (24 * 60 * 60 * 365);
    unix_ts %= 24 * 60 * 60 * 365;

    // The year we get out of this calculation is the number of years since 1970.
    let year_u16 = year as u16 + 1970;

    // We need to compensate for the number of leap years when calculating the day of year.
    let num_leaps = calc_num_leap_years(year_u16);
    let doy = unix_ts / (24 * 60 * 60) - num_leaps + 1;
    unix_ts %= 24 * 60 * 60;

    let hour = unix_ts / (60 * 60);
    unix_ts %= 60 * 60;
    let minute = unix_ts / 60;
    let seconds = unix_ts % 60;

    (year + 1970, doy, hour, minute, seconds)
}

fn calc_num_leap_years(year: u16) -> u32 {
    let mut num_leaps = 0u32;

    // We only care about the number of leap years since the Unix epoch and we know the first leap
    // year after the epoch was in 1972.
    for leap in 1972..year {
        if (leap % 400 == 0 || leap % 100 != 0) && leap % 4 == 0 {
            num_leaps += 1;
        }
    }
    num_leaps
}

fn is_leap_year(year: u16) -> bool {
    (year % 400 == 0 || year % 100 != 0) && year % 4 == 0
}

/// Calculates the year, month, and day based off the data from RTC.
pub fn to_date(year: u16, doy: u16) -> (u16, u16, u16) {
    let mut days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = is_leap_year(year);
    if is_leap {
        days_in_month[1] += 1;
    }

    let mut month = 0;
    let mut day = doy;
    for (iter_month, num_days) in days_in_month.iter().enumerate() {
        if day > *num_days {
            day -= num_days;
        } else {
            month = iter_month + 1;
            break;
        }
    }

    (year, month as u16, day)
}
