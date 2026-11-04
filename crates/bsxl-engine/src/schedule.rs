//! Scheduled refresh through Windows Task Scheduler.
//!
//! The task runs `BoilerSteamXL.exe --scan --scheduled`: a headless discovery that
//! updates the review queue and can notify. It never imports. `StartWhenAvailable`
//! makes Windows run a missed refresh once the PC is back on.

use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime, Time, Weekday};

pub const TASK_NAME: &str = "BoilerSteamXL Library Refresh";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleMode {
    Manual,
    Daily,
    Weekly,
    /// Every `interval_days` days.
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    pub mode: ScheduleMode,
    /// Local time, "HH:MM".
    pub time: String,
    /// For weekly: "monday".."sunday".
    pub days: Vec<String>,
    pub interval_days: u32,
    pub run_missed: bool,
    pub scan_on_startup: bool,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            mode: ScheduleMode::Manual,
            time: "09:00".into(),
            days: vec!["monday".into()],
            interval_days: 3,
            run_missed: true,
            scan_on_startup: false,
        }
    }
}

const WEEKDAYS: [(&str, Weekday, &str); 7] = [
    ("monday", Weekday::Monday, "Monday"),
    ("tuesday", Weekday::Tuesday, "Tuesday"),
    ("wednesday", Weekday::Wednesday, "Wednesday"),
    ("thursday", Weekday::Thursday, "Thursday"),
    ("friday", Weekday::Friday, "Friday"),
    ("saturday", Weekday::Saturday, "Saturday"),
    ("sunday", Weekday::Sunday, "Sunday"),
];

pub fn parse_time(text: &str) -> Result<Time, String> {
    let (h, m) = text.trim().split_once(':').ok_or("Use HH:MM, for example 09:00.")?;
    let h: u8 = h.parse().map_err(|_| "Use HH:MM, for example 09:00.")?;
    let m: u8 = m.parse().map_err(|_| "Use HH:MM, for example 09:00.")?;
    Time::from_hms(h, m, 0).map_err(|_| "Use HH:MM, for example 09:00.".to_string())
}

impl Schedule {
    pub fn validate(&self) -> Result<(), String> {
        parse_time(&self.time)?;
        if self.mode == ScheduleMode::Weekly && self.weekdays().is_empty() {
            return Err("Choose at least one day.".into());
        }
        if self.mode == ScheduleMode::Custom && !(1..=365).contains(&self.interval_days) {
            return Err("Choose between 1 and 365 days.".into());
        }
        Ok(())
    }

    fn weekdays(&self) -> Vec<(Weekday, &'static str)> {
        WEEKDAYS
            .iter()
            .filter(|(key, _, _)| self.days.iter().any(|d| d.eq_ignore_ascii_case(key)))
            .map(|(_, day, xml)| (*day, *xml))
            .collect()
    }
}

/// The next run after `now` (local time), or `None` for manual refresh.
pub fn next_run(schedule: &Schedule, now: OffsetDateTime) -> Option<OffsetDateTime> {
    let at = parse_time(&schedule.time).ok()?;
    let today = now.replace_time(at);
    match schedule.mode {
        ScheduleMode::Manual => None,
        ScheduleMode::Daily => Some(if today > now { today } else { today + Duration::days(1) }),
        // ponytail: assumes the interval starts today (as when the task is saved); exact
        // phase would need the task's stored StartBoundary.
        ScheduleMode::Custom => {
            Some(if today > now { today } else { today + Duration::days(i64::from(schedule.interval_days.max(1))) })
        }
        ScheduleMode::Weekly => {
            let days = schedule.weekdays();
            (0..8)
                .map(|offset| today + Duration::days(offset))
                .find(|candidate| *candidate > now && days.iter().any(|(d, _)| *d == candidate.weekday()))
        }
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Task Scheduler XML for the schedule. `start_date` is "YYYY-MM-DD".
pub fn task_xml(schedule: &Schedule, exe: &str, start_date: &str) -> Result<String, String> {
    schedule.validate()?;
    let at = parse_time(&schedule.time)?;
    let start = format!("{start_date}T{:02}:{:02}:00", at.hour(), at.minute());
    let trigger_body = match schedule.mode {
        ScheduleMode::Manual => return Err("Manual refresh does not need a scheduled task.".into()),
        ScheduleMode::Daily => "<ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>".to_string(),
        ScheduleMode::Custom => format!("<ScheduleByDay><DaysInterval>{}</DaysInterval></ScheduleByDay>", schedule.interval_days),
        ScheduleMode::Weekly => {
            let days: String = schedule.weekdays().iter().map(|(_, xml)| format!("<{xml} />")).collect();
            format!("<ScheduleByWeek><DaysOfWeek>{days}</DaysOfWeek><WeeksInterval>1</WeeksInterval></ScheduleByWeek>")
        }
    };
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>BoilerSteamXL: looks for new games and adds them to the review queue. It never imports anything without your approval.</Description>
  </RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>{start}</StartBoundary>
      <Enabled>true</Enabled>
      {trigger_body}
    </CalendarTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>{missed}</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <ExecutionTimeLimit>PT1H</ExecutionTimeLimit>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>--scan --scheduled</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        missed = schedule.run_missed,
        exe = xml_escape(exe),
    ))
}

#[cfg(windows)]
fn schtasks(args: &[&str]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("schtasks.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Could not run schtasks: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Creates or replaces the task (or removes it for manual refresh).
#[cfg(windows)]
pub fn apply(schedule: &Schedule, exe: &str) -> Result<(), String> {
    if schedule.mode == ScheduleMode::Manual {
        return remove();
    }
    let today = OffsetDateTime::now_local()
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
        .format(time::macros::format_description!("[year]-[month]-[day]"))
        .map_err(|e| e.to_string())?;
    let xml = task_xml(schedule, exe, &today)?;
    let file = crate::data_dir().join("refresh-task.xml");
    let mut bytes = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    let path = file.to_string_lossy().to_string();
    schtasks(&["/Create", "/TN", TASK_NAME, "/XML", &path, "/F"]).map(|_| ())
}

#[cfg(windows)]
pub fn remove() -> Result<(), String> {
    if !is_installed() {
        return Ok(());
    }
    schtasks(&["/Delete", "/TN", TASK_NAME, "/F"]).map(|_| ())
}

#[cfg(windows)]
pub fn is_installed() -> bool {
    schtasks(&["/Query", "/TN", TASK_NAME]).is_ok()
}

#[cfg(not(windows))]
pub fn apply(_schedule: &Schedule, _exe: &str) -> Result<(), String> {
    Err("Scheduled refresh uses Windows Task Scheduler.".into())
}
#[cfg(not(windows))]
pub fn remove() -> Result<(), String> {
    Ok(())
}
#[cfg(not(windows))]
pub fn is_installed() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn daily_task_xml() {
        let s = Schedule { mode: ScheduleMode::Daily, time: "07:30".into(), ..Schedule::default() };
        let xml = task_xml(&s, r"C:\Apps\BoilerSteamXL.exe", "2026-11-04").unwrap_or_default();
        assert!(xml.contains("<StartBoundary>2026-11-04T07:30:00</StartBoundary>"));
        assert!(xml.contains("<DaysInterval>1</DaysInterval>"));
        assert!(xml.contains("<Arguments>--scan --scheduled</Arguments>"));
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(xml.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
    }

    #[test]
    fn weekly_task_xml_and_escaping() {
        let s = Schedule {
            mode: ScheduleMode::Weekly,
            days: vec!["monday".into(), "friday".into()],
            run_missed: false,
            ..Schedule::default()
        };
        let xml = task_xml(&s, r"D:\PLUG&PLAY\BoilerSteamXL.exe", "2026-11-04").unwrap_or_default();
        assert!(xml.contains("<Monday /><Friday />"));
        assert!(xml.contains(r"D:\PLUG&amp;PLAY\BoilerSteamXL.exe"));
        assert!(xml.contains("<StartWhenAvailable>false</StartWhenAvailable>"));
    }

    #[test]
    fn manual_has_no_task_and_bad_input_is_rejected() {
        assert!(task_xml(&Schedule::default(), "x", "2026-11-04").is_err());
        let bad = Schedule { mode: ScheduleMode::Daily, time: "25:00".into(), ..Schedule::default() };
        assert!(bad.validate().is_err());
        let none = Schedule { mode: ScheduleMode::Weekly, days: vec![], ..Schedule::default() };
        assert!(none.validate().is_err());
    }

    #[test]
    fn next_run_times() {
        // 2026-11-04 is a Wednesday.
        let now = datetime!(2026-11-04 10:00 UTC);
        let daily = Schedule { mode: ScheduleMode::Daily, time: "09:00".into(), ..Schedule::default() };
        assert_eq!(next_run(&daily, now), Some(datetime!(2026-11-05 09:00 UTC)));
        let later = Schedule { mode: ScheduleMode::Daily, time: "18:00".into(), ..Schedule::default() };
        assert_eq!(next_run(&later, now), Some(datetime!(2026-11-04 18:00 UTC)));
        let weekly = Schedule { mode: ScheduleMode::Weekly, days: vec!["monday".into()], ..Schedule::default() };
        assert_eq!(next_run(&weekly, now), Some(datetime!(2026-11-09 09:00 UTC)));
        assert_eq!(next_run(&Schedule::default(), now), None);
    }
}
