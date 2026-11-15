use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::GetLocalTime;

use core::{Clock, PowerSource, PowerState};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> (u8, u8) {
        let t = unsafe { GetLocalTime() };
        (t.wHour as u8, t.wMinute as u8)
    }
}

pub struct SystemPower;

impl PowerSource for SystemPower {
    fn read(&self) -> Option<PowerState> {
        let mut sps = SYSTEM_POWER_STATUS::default();
        let ok = unsafe { GetSystemPowerStatus(&mut sps) }.is_ok() && sps.BatteryLifePercent <= 100;
        if ok {
            Some(PowerState {
                percent: sps.BatteryLifePercent,
                on_ac: sps.ACLineStatus == 1,
            })
        } else {
            None
        }
    }
}

pub fn ms_until_next_minute_from(sec: u16, ms: u16) -> u32 {
    let sec = (sec as i64).clamp(0, 59);
    let ms = (ms as i64).clamp(0, 999);
    let remaining = (60 - sec) * 1000 - ms;
    (remaining as u32).clamp(100, 60_000)
}

pub fn ms_until_next_minute() -> u32 {
    let t = unsafe { GetLocalTime() };
    ms_until_next_minute_from(t.wSecond, t.wMilliseconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ms_until_next_minute_from_computes_and_clamps() {
        assert_eq!(ms_until_next_minute_from(0, 0), 60_000);
        assert_eq!(ms_until_next_minute_from(30, 500), 29_500);
        assert_eq!(ms_until_next_minute_from(59, 0), 1_000);
        assert_eq!(ms_until_next_minute_from(59, 950), 100);
        assert_eq!(ms_until_next_minute_from(59, 999), 100);
        assert_eq!(ms_until_next_minute_from(60, 999), 100);
    }

    #[test]
    fn ms_until_next_minute_within_bounds() {
        let ms = ms_until_next_minute();
        assert!((100..=60_000).contains(&ms), "ms was {ms}");
    }

    #[test]
    fn system_clock_returns_valid_hour_and_minute() {
        let (h, m) = SystemClock.now();
        assert!(h < 24, "hour out of range: {h}");
        assert!(m < 60, "minute out of range: {m}");
    }

    #[test]
    fn system_power_returns_valid_percentage_when_present() {
        if let Some(state) = SystemPower.read() {
            assert!(state.percent <= 100, "percent out of range: {}", state.percent);
        }
    }
}
