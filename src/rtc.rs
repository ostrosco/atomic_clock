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
