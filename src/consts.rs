// A list of constants used to index into the WWVB frame.
pub const FRM: usize = 0;
pub const MINUTE_40: usize = 1;
pub const MINUTE_20: usize = 2;
pub const MINUTE_10: usize = 3;
pub const MINUTE_8: usize = 5;
pub const MINUTE_4: usize = 6;
pub const MINUTE_2: usize = 7;
pub const MINUTE_1: usize = 8;
pub const P1: usize = 9;
pub const HOUR_20: usize = 12;
pub const HOUR_10: usize = 13;
pub const HOUR_8: usize = 15;
pub const HOUR_4: usize = 16;
pub const HOUR_2: usize = 17;
pub const HOUR_1: usize = 18;
pub const P2: usize = 19;
pub const DOY_200: usize = 22;
pub const DOY_100: usize = 23;
pub const DOY_80: usize = 25;
pub const DOY_40: usize = 26;
pub const DOY_20: usize = 27;
pub const DOY_10: usize = 28;
pub const P3: usize = 29;
pub const DOY_8: usize = 30;
pub const DOY_4: usize = 31;
pub const DOY_2: usize = 32;
pub const DOY_1: usize = 33;
pub const DUT1_PLUS1: usize = 36;
pub const DUT1_MINUS: usize = 37;
pub const DUT1_PLUS2: usize = 38;
pub const P4: usize = 39;
pub const DUT1_08: usize = 40;
pub const DUT1_04: usize = 41;
pub const DUT1_02: usize = 42;
pub const DUT1_01: usize = 43;
pub const YEAR_80: usize = 45;
pub const YEAR_40: usize = 46;
pub const YEAR_20: usize = 47;
pub const YEAR_10: usize = 48;
pub const P5: usize = 49;
pub const YEAR_8: usize = 50;
pub const YEAR_4: usize = 51;
pub const YEAR_2: usize = 52;
pub const YEAR_1: usize = 53;
pub const LEAP_YEAR: usize = 55;
pub const LEAP_SECOND: usize = 56;
pub const DST_BIT1: usize = 57;
pub const DST_BIT0: usize = 58;
pub const P0: usize = 59;

// The sysclock is set to run at 48MHz, so we're prescaling down to a 10kHz clock and setting the
// counter to 10000.
pub const ARR: u16 = 10000;
pub const PRESC: u16 = 4799;

// Bounds for accepting a signal as one of the three values that WWVB can generate.
pub const SYNC_MIN: u16 = 1000;
pub const SYNC_MAX: u16 = 3000;
pub const ONE_MIN: u16 = 4000;
pub const ONE_MAX: u16 = 6000;
pub const ZERO_MIN: u16 = 7000;
pub const ZERO_MAX: u16 = 9000;
