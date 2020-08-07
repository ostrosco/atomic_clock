/// A series of utility functions for handling time.

pub struct Timestamp {
    pub year: u16,
    pub doy: u16,
    pub hour: u16,
    pub minute: u16,
    pub seconds: u16,
}

pub struct Date {
    pub year: u16,
    pub month: u16,
    pub day: u16,
}

impl Timestamp {
    pub fn new(
        year: u16,
        doy: u16,
        hour: u16,
        minute: u16,
        seconds: u16,
    ) -> Self {
        Self {
            year,
            doy,
            hour,
            minute,
            seconds,
        }
    }

    pub fn to_unix(&self) -> u32 {
        let year_u32: u32 = self.year.into();
        let doy_u32: u32 = self.doy.into();
        let hour_u32: u32 = self.hour.into();
        let minute_u32: u32 = self.minute.into();
        let seconds_u32: u32 = self.seconds.into();

        let secs_in_day = 86400;
        let days_in_year = 365;
        let num_leaps = calc_num_leap_years(self.year);

        ((year_u32 - 1970) * days_in_year + doy_u32 - 1) * secs_in_day
            + hour_u32 * 60 * 60
            + minute_u32 * 60
            + num_leaps * secs_in_day
            + seconds_u32
    }

    pub fn from_unix(unix_ts: u32) -> Self {
        let mut unix_ts = unix_ts;
        let year = unix_ts / (24 * 60 * 60 * 365);
        unix_ts %= 24 * 60 * 60 * 365;

        // The year we get out of this calculation is the number of years since 1970.
        let year_u16 = year as u16 + 1970;

        // We need to compensate for the number of leap years when calculating the day of year.
        let num_leaps = calc_num_leap_years(year_u16);
        let doy = (unix_ts / (24 * 60 * 60) - num_leaps + 1) as u16;
        unix_ts %= 24 * 60 * 60;

        let hour = (unix_ts / (60 * 60)) as u16;
        unix_ts %= 60 * 60;
        let minute = (unix_ts / 60) as u16;
        let seconds = (unix_ts % 60) as u16;

        Self {
            year: year_u16,
            doy,
            hour,
            minute,
            seconds,
        }
    }

    /// Calculates the year, month, and day based off the data from RTC.
    pub fn to_date(&self) -> Date {
        let mut days_in_month =
            [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let is_leap = is_leap_year(self.year);
        if is_leap {
            days_in_month[1] += 1;
        }

        let mut month = 0;
        let mut day = self.doy;
        for (iter_month, num_days) in days_in_month.iter().enumerate() {
            if day > *num_days {
                day -= num_days;
            } else {
                month = iter_month + 1;
                break;
            }
        }

        Date {
            year: self.year,
            month: month as u16,
            day,
        }
    }
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
