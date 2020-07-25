/// An enumeration of the fields in the WWVB signal. Fields which are unused are not listed.
use crate::consts;

pub enum WWVBError {
    InvalidSync,
    UnknownSignal,
}

pub fn calc_minute(frame: &[u16; 60]) -> u16 {
    frame[consts::MINUTE_40] * 40
        + frame[consts::MINUTE_20] * 20
        + frame[consts::MINUTE_10] * 10
        + frame[consts::MINUTE_8] * 8
        + frame[consts::MINUTE_4] * 4
        + frame[consts::MINUTE_2] * 2
        + frame[consts::MINUTE_1]
}

pub fn calc_hour(frame: &[u16; 60]) -> u16 {
    frame[consts::HOUR_20] * 20
        + frame[consts::HOUR_10] * 10
        + frame[consts::HOUR_8] * 8
        + frame[consts::HOUR_4] * 4
        + frame[consts::HOUR_2] * 2
        + frame[consts::HOUR_1]
}

pub fn calc_doy(frame: &[u16; 60]) -> u16 {
    frame[consts::DOY_200] * 200
        + frame[consts::DOY_100] * 100
        + frame[consts::DOY_80] * 80
        + frame[consts::DOY_40] * 40
        + frame[consts::DOY_20] * 20
        + frame[consts::DOY_10] * 10
        + frame[consts::DOY_8] * 8
        + frame[consts::DOY_4] * 4
        + frame[consts::DOY_2] * 2
        + frame[consts::DOY_1]
}

pub fn calc_year(frame: &[u16; 60]) -> u16 {
    frame[consts::YEAR_80] * 80
        + frame[consts::YEAR_40] * 40
        + frame[consts::YEAR_20] * 20
        + frame[consts::YEAR_10] * 10
        + frame[consts::YEAR_8] * 8
        + frame[consts::YEAR_4] * 4
        + frame[consts::YEAR_2] * 2
        + frame[consts::YEAR_1]
}

pub fn handle_bit(duty: u16, index: usize) -> Result<u16, WWVBError> {
    if duty > consts::SYNC_MIN && duty < consts::SYNC_MAX {
        // We only expect sync signals at the appropriate positions in the frame. If we
        // get a sync outside these bounds, we must have missed something and we should
        // assume that our current data is borked and need to resync.
        if index == consts::FRM
            || index == consts::P1
            || index == consts::P2
            || index == consts::P3
            || index == consts::P4
            || index == consts::P5
            || index == consts::P0
        {
            Ok(2)
        } else {
            Err(WWVBError::InvalidSync)
        }
    } else if duty > consts::ONE_MIN && duty < consts::ONE_MAX {
        Ok(1)
    } else if duty > consts::ZERO_MIN && duty < consts::ZERO_MAX {
        Ok(0)
    } else {
        // If this happens, we've received a signal that does not have a duty that meshed with any
        // known signal. It's outside our tolerances for error, so just report the error and
        // resync.
        Err(WWVBError::UnknownSignal)
    }
}

pub fn is_leap_year(&frame: &[u16; 60]) -> bool {
    frame[consts::LEAP_YEAR] == 1
}

/// Calculates the year, month, and day based off the data from WWVB. Due to current limitations
/// with the WWVB data stream, there's no way to extrapolate what century we're in. As we don't
/// expect this clock to last another 80 years, the century has been hard-coded to 2000.
pub fn to_date(year: u16, doy: u16, leap_year: bool) -> (u16, u16, u16) {
    let mut days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if leap_year {
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

    (year + 2000, month as u16, day)
}
