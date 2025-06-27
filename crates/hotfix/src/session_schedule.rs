use crate::config::ScheduleConfig;
use crate::error::SessionError;
use chrono::{DateTime, Datelike, NaiveTime, Utc, Weekday};
use chrono_tz::Tz;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum SessionSchedule {
    NonStop,
    Daily {
        start_time: NaiveTime,
        end_time: NaiveTime,
        timezone: Tz,
    },
    Weekdays {
        start_time: NaiveTime,
        end_time: NaiveTime,
        weekdays: Vec<Weekday>,
        timezone: Tz,
    },
    Weekly {
        start_day: Weekday,
        start_time: NaiveTime,
        end_day: Weekday,
        end_time: NaiveTime,
        timezone: Tz,
    },
}

#[allow(dead_code)]
impl SessionSchedule {
    pub fn is_active_at(&self, datetime: &DateTime<Utc>) -> bool {
        match self {
            SessionSchedule::NonStop => true,
            SessionSchedule::Daily {
                start_time,
                end_time,
                timezone,
            } => {
                let adjusted_datetime = datetime.with_timezone(timezone);
                Self::check_daily_schedule(&adjusted_datetime, start_time, end_time)
            }
            SessionSchedule::Weekdays {
                weekdays,
                start_time,
                end_time,
                timezone,
            } => {
                let adjusted_datetime = datetime.with_timezone(timezone);
                Self::check_weekdays_schedule(&adjusted_datetime, weekdays, start_time, end_time)
            }
            SessionSchedule::Weekly { .. } => false,
        }
    }

    fn check_daily_schedule(
        datetime: &DateTime<Tz>,
        start_time: &NaiveTime,
        end_time: &NaiveTime,
    ) -> bool {
        if start_time < end_time {
            &datetime.time() >= start_time && &datetime.time() < end_time
        } else {
            &datetime.time() >= start_time || &datetime.time() < end_time
        }
    }

    fn check_weekdays_schedule(
        datetime: &DateTime<Tz>,
        weekdays: &[Weekday],
        start_time: &NaiveTime,
        end_time: &NaiveTime,
    ) -> bool {
        let time_of_day = &datetime.time();

        if start_time < end_time {
            // schedule doesn't span midnight
            weekdays.contains(&datetime.weekday())
                && time_of_day >= start_time
                && time_of_day < end_time
        } else {
            // schedule spans midnight
            if time_of_day >= end_time && time_of_day < start_time {
                return false;
            }

            let target_day = if time_of_day >= start_time {
                datetime.weekday()
            } else {
                datetime.weekday().pred()
            };
            weekdays.contains(&target_day)
        }
    }

    #[allow(unused_variables)]
    fn check_weekly_schedule(
        datetime: &DateTime<Utc>,
        start_day: Weekday,
        end_day: Weekday,
    ) -> bool {
        // TODO: implement this
        false
    }
}

impl TryFrom<ScheduleConfig> for SessionSchedule {
    type Error = SessionError;

    fn try_from(config: ScheduleConfig) -> Result<Self, Self::Error> {
        match config {
            // NonStop: no configuration provided
            ScheduleConfig {
                start_time: None,
                end_time: None,
                start_day: None,
                end_day: None,
                weekdays,
                timezone: None,
            } if weekdays.is_empty() => Ok(SessionSchedule::NonStop),

            // Daily/weekdays sessions
            ScheduleConfig {
                start_time: Some(start),
                end_time: Some(end),
                start_day: None,
                end_day: None,
                weekdays,
                timezone,
            } => {
                if weekdays.is_empty() {
                    if start == end {
                        Ok(SessionSchedule::NonStop)
                    } else {
                        Ok(SessionSchedule::Daily {
                            start_time: start,
                            end_time: end,
                            timezone: timezone.unwrap_or(Tz::UTC),
                        })
                    }
                } else if start == end {
                    Err(SessionError::InvalidSchedule(
                        "Start and end times cannot be equal when weekdays is set".to_string(),
                    ))
                } else {
                    Ok(SessionSchedule::Weekdays {
                        start_time: start,
                        end_time: end,
                        weekdays,
                        timezone: timezone.unwrap_or(Tz::UTC),
                    })
                }
            }

            // Weekly sessions
            ScheduleConfig {
                start_day: Some(start_day),
                start_time: Some(start),
                end_day: Some(end_day),
                end_time: Some(end),
                weekdays,
                timezone,
            } => {
                // Weekdays should be empty for weekly sessions
                if !weekdays.is_empty() {
                    return Err(SessionError::InvalidSchedule(
                        "Weekly sessions cannot have weekdays specified".to_string(),
                    ));
                }

                Ok(SessionSchedule::Weekly {
                    start_day,
                    start_time: start,
                    end_day,
                    end_time: end,
                    timezone: timezone.unwrap_or(Tz::UTC),
                })
            }

            // Invalid combinations
            _ => Err(SessionError::InvalidSchedule(
                "Invalid schedule configuration: incomplete or conflicting parameters".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveTime, Weekday};

    #[test]
    fn test_active_at_non_stop_schedule() {
        // non-stop schedules are always active
        let schedule = SessionSchedule::NonStop;
        assert!(schedule.is_active_at(&Utc::now()))
    }

    #[test]
    fn test_active_at_daily_schedule_utc() {
        let schedule = SessionSchedule::Daily {
            start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            timezone: Tz::UTC,
        };

        // just before start time (8:59:59)
        let before_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
                .unwrap()
                .and_hms_opt(8, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&before_start));

        // just after start time (9:00:01)
        let after_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
                .unwrap()
                .and_hms_opt(9, 0, 1)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&after_start));

        // in the middle (13:00:00)
        let middle = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
                .unwrap()
                .and_hms_opt(13, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&middle));

        // just before end time (16:59:59)
        let before_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
                .unwrap()
                .and_hms_opt(16, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&before_end));

        // at end time (17:00:00) - we expect false at exactly the end time (non-inclusive)
        let at_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
                .unwrap()
                .and_hms_opt(17, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&at_end));

        // after end time (17:00:01)
        let after_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
                .unwrap()
                .and_hms_opt(17, 0, 1)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&after_end));
    }

    #[test]
    fn test_active_at_daily_schedule_london() {
        // we'll use 27/06/2025 as the date
        // London is an hour ahead of UTC
        let schedule = SessionSchedule::Daily {
            start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            timezone: Tz::Europe__London,
        };

        let before_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(7, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&before_start));

        // 8AM UTC is 9AM London time, so already in session
        let at_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(8, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&at_start));

        // 4PM UTC is 5PM London time, so already out of session
        let at_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(16, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&at_end));
    }

    #[test]
    fn test_active_at_daily_schedule_london_end_before_start() {
        // we'll use 27/06/2025 as the date
        // London is an hour ahead of UTC
        let schedule_1 = SessionSchedule::Daily {
            start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
            timezone: Tz::Europe__London,
        };

        let before_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(7, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(!schedule_1.is_active_at(&before_start));

        let at_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(8, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(schedule_1.is_active_at(&at_start));

        let before_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(0, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(schedule_1.is_active_at(&before_end));

        let at_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(1, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule_1.is_active_at(&at_end));
    }

    #[test]
    fn test_active_at_daily_schedule_london_end_before_start_tz_crossing_midnight() {
        // we'll use 27/06/2025 as the date
        // London is an hour ahead of UTC
        let schedule_1 = SessionSchedule::Daily {
            start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(0, 30, 0).unwrap(),
            timezone: Tz::Europe__London,
        };

        let before_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(7, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(!schedule_1.is_active_at(&before_start));

        let at_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(8, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(schedule_1.is_active_at(&at_start));

        let before_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(23, 29, 59)
                .unwrap(),
            Utc,
        );
        assert!(schedule_1.is_active_at(&before_end));

        let at_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 27)
                .unwrap()
                .and_hms_opt(23, 30, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule_1.is_active_at(&at_end));
    }

    #[test]
    fn test_active_at_weekdays_schedule_utc() {
        // Monday to Friday, 9AM to 5PM UTC
        let schedule = SessionSchedule::Weekdays {
            start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            weekdays: vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ],
            timezone: Tz::UTC,
        };

        // Monday 8:59:59 - before start time
        let monday_before_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 30) // Monday
                .unwrap()
                .and_hms_opt(8, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&monday_before_start));

        // Monday 9:00:01 - after start time
        let monday_after_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 30) // Monday
                .unwrap()
                .and_hms_opt(9, 0, 1)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&monday_after_start));

        // Friday 16:59:59 - just before end time on a valid day
        let friday_before_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 4) // Friday
                .unwrap()
                .and_hms_opt(16, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&friday_before_end));

        // Friday 17:00:00 - at end time on a valid day (exclusive end)
        let friday_at_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 4) // Friday
                .unwrap()
                .and_hms_opt(17, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&friday_at_end));

        // Saturday 12:00:00 - middle of day on weekend
        let saturday = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 5) // Saturday
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&saturday));
    }

    #[test]
    fn test_active_at_weekdays_schedule_london() {
        // Monday to Friday, 9AM to 5PM London time
        // During summer (June), London is UTC+1
        let schedule = SessionSchedule::Weekdays {
            start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            weekdays: vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ],
            timezone: Tz::Europe__London,
        };

        // Monday 7:59:59 UTC = 8:59:59 London - before start time
        let monday_before_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 30) // Monday
                .unwrap()
                .and_hms_opt(7, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&monday_before_start));

        // Monday 8:00:01 UTC = 9:00:01 London - after start time
        let monday_after_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 30) // Monday
                .unwrap()
                .and_hms_opt(8, 0, 1)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&monday_after_start));

        // Friday 15:59:59 UTC = 16:59:59 London - just before end time
        let friday_before_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 4) // Friday
                .unwrap()
                .and_hms_opt(15, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&friday_before_end));

        // Friday 16:00:00 UTC = 17:00:00 London - at end time (exclusive)
        let friday_at_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 4) // Friday
                .unwrap()
                .and_hms_opt(16, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&friday_at_end));
    }

    #[test]
    fn test_active_at_weekdays_schedule_newyork() {
        // Monday to Friday, 9:30AM to 4PM New York time
        // During summer (June), New York is UTC-4
        let schedule = SessionSchedule::Weekdays {
            start_time: NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
            weekdays: vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ],
            timezone: Tz::America__New_York,
        };

        // Monday 13:29:59 UTC = 9:29:59 NY - just before start time
        let monday_before_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 30) // Monday
                .unwrap()
                .and_hms_opt(13, 29, 59)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&monday_before_start));

        // Monday 13:30:01 UTC = 9:30:01 NY - just after start time
        let monday_after_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 30) // Monday
                .unwrap()
                .and_hms_opt(13, 30, 1)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&monday_after_start));

        // Tuesday 19:59:59 UTC = 15:59:59 NY - just before end time
        let tuesday_before_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 1) // Tuesday
                .unwrap()
                .and_hms_opt(19, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&tuesday_before_end));

        // Tuesday 20:00:00 UTC = 16:00:00 NY - at end time (exclusive)
        let tuesday_at_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 1) // Tuesday
                .unwrap()
                .and_hms_opt(20, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&tuesday_at_end));
    }

    #[test]
    fn test_active_at_weekdays_schedule_sydney_crossing_midnight() {
        // Monday to Friday, 10PM to 6AM Sydney time (crosses midnight)
        // In June, Sydney is UTC+10
        let schedule = SessionSchedule::Weekdays {
            start_time: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            weekdays: vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ],
            timezone: Tz::Australia__Sydney,
        };

        // Monday 11:59:59 UTC = 21:59:59 Sydney - just before start time
        let monday_before_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 30) // Monday
                .unwrap()
                .and_hms_opt(11, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&monday_before_start));

        // Monday 12:00:01 UTC = 22:00:01 Sydney - just after start time
        let monday_after_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 30) // Monday
                .unwrap()
                .and_hms_opt(12, 0, 1)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&monday_after_start));

        // Tuesday 19:59:59 UTC = 5:59:59 Sydney Wednesday - just before end time
        let tuesday_before_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 1) // Tuesday
                .unwrap()
                .and_hms_opt(19, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&tuesday_before_end));

        // Tuesday 20:00:00 UTC = 6:00:00 Sydney Wednesday - at end time (exclusive)
        let tuesday_at_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 1) // Tuesday
                .unwrap()
                .and_hms_opt(20, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&tuesday_at_end));

        // Wednesday 10:00:00 UTC = 20:00:00 Sydney - during inactive period
        let wednesday_inactive = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 2) // Wednesday
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&wednesday_inactive));
    }

    #[test]
    fn test_active_at_weekdays_schedule_only_weekend() {
        // Weekend schedule (Saturday and Sunday), 10AM to 4PM UTC
        let schedule = SessionSchedule::Weekdays {
            start_time: NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
            weekdays: vec![Weekday::Sat, Weekday::Sun],
            timezone: Tz::UTC,
        };

        // Friday 12:00:00 - should be inactive (weekday)
        let friday = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 4) // Friday
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&friday));

        // Saturday 9:59:59 - before start time on weekend
        let saturday_before_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 5) // Saturday
                .unwrap()
                .and_hms_opt(9, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&saturday_before_start));

        // Saturday 10:00:01 - after start time on weekend
        let saturday_after_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 5) // Saturday
                .unwrap()
                .and_hms_opt(10, 0, 1)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&saturday_after_start));

        // Sunday 15:59:59 - before end time on weekend
        let sunday_before_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 6) // Sunday
                .unwrap()
                .and_hms_opt(15, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&sunday_before_end));
    }

    #[test]
    fn test_active_at_weekdays_schedule_overnight_crossing_weekdays() {
        // Monday to Thursday nights, 10PM to 6AM London time
        // During summer (June), London is UTC+1
        let schedule = SessionSchedule::Weekdays {
            start_time: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            weekdays: vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu],
            timezone: Tz::Europe__London,
        };

        // Monday 20:59:59 UTC = 21:59:59 London - just before start time
        let monday_before_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 30) // Monday
                .unwrap()
                .and_hms_opt(20, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&monday_before_start));

        // Monday 21:00:01 UTC = 22:00:01 London - just after start time
        let monday_after_start = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 6, 30) // Monday
                .unwrap()
                .and_hms_opt(21, 0, 1)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&monday_after_start));

        // Tuesday 4:59:59 UTC = 5:59:59 London - just before end time, still Monday's session
        let tuesday_before_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 1) // Tuesday
                .unwrap()
                .and_hms_opt(4, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&tuesday_before_end));

        // Tuesday 5:00:00 UTC = 6:00:00 London - at end time, should be inactive
        let tuesday_at_end = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 1) // Tuesday
                .unwrap()
                .and_hms_opt(5, 0, 0)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&tuesday_at_end));

        // Friday 21:00:01 UTC = 22:00:01 London - after start time but on Friday which is excluded
        let friday_night = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 4) // Friday
                .unwrap()
                .and_hms_opt(21, 0, 1)
                .unwrap(),
            Utc,
        );
        assert!(!schedule.is_active_at(&friday_night));

        // Thursday 21:00:01 UTC = 22:00:01 London - Thursday night session should be active
        let thursday_night = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 3) // Thursday
                .unwrap()
                .and_hms_opt(21, 0, 1)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&thursday_night));

        // Friday 4:59:59 UTC = 5:59:59 London - still Thursday's session ending on Friday morning
        let friday_morning = DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2025, 7, 4) // Friday
                .unwrap()
                .and_hms_opt(4, 59, 59)
                .unwrap(),
            Utc,
        );
        assert!(schedule.is_active_at(&friday_morning));
    }

    #[test]
    fn test_into_non_stop_no_config() {
        let config = ScheduleConfig {
            start_time: None,
            end_time: None,
            start_day: None,
            end_day: None,
            weekdays: vec![],
            timezone: None,
        };

        let schedule = SessionSchedule::try_from(config).unwrap();
        assert!(matches!(schedule, SessionSchedule::NonStop));
    }

    #[test]
    fn test_into_non_stop_equal_times() {
        let time = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        let config = ScheduleConfig {
            start_time: Some(time),
            end_time: Some(time),
            start_day: None,
            end_day: None,
            weekdays: vec![],
            timezone: None,
        };

        let schedule = SessionSchedule::try_from(config).unwrap();
        assert!(matches!(schedule, SessionSchedule::NonStop));
    }

    #[test]
    fn test_into_daily_session() {
        let config = ScheduleConfig {
            start_time: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            end_time: Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            start_day: None,
            end_day: None,
            weekdays: vec![],
            timezone: None,
        };

        let schedule = SessionSchedule::try_from(config).unwrap();
        match schedule {
            SessionSchedule::Daily {
                start_time,
                end_time,
                timezone,
            } => {
                assert_eq!(start_time, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
                assert_eq!(end_time, NaiveTime::from_hms_opt(17, 0, 0).unwrap());
                assert_eq!(timezone, Tz::UTC);
            }
            _ => panic!("Expected Daily schedule"),
        }
    }

    #[test]
    fn test_into_weekdays_session() {
        let config = ScheduleConfig {
            start_time: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            end_time: Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            start_day: None,
            end_day: None,
            weekdays: vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ],
            timezone: Some(Tz::Europe__London),
        };

        let schedule = SessionSchedule::try_from(config).unwrap();
        match schedule {
            SessionSchedule::Weekdays {
                start_time,
                end_time,
                weekdays,
                timezone,
            } => {
                assert_eq!(start_time, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
                assert_eq!(end_time, NaiveTime::from_hms_opt(17, 0, 0).unwrap());
                assert_eq!(
                    weekdays,
                    vec![
                        Weekday::Mon,
                        Weekday::Tue,
                        Weekday::Wed,
                        Weekday::Thu,
                        Weekday::Fri
                    ]
                );
                assert_eq!(timezone, Tz::Europe__London);
            }
            _ => panic!("Expected Weekdays schedule"),
        }
    }

    #[test]
    fn test_into_weekly_session() {
        let config = ScheduleConfig {
            start_time: Some(NaiveTime::from_hms_opt(18, 0, 0).unwrap()),
            end_time: Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            start_day: Some(Weekday::Sun),
            end_day: Some(Weekday::Fri),
            weekdays: vec![],
            timezone: None,
        };

        let schedule = SessionSchedule::try_from(config).unwrap();
        match schedule {
            SessionSchedule::Weekly {
                start_day,
                start_time,
                end_day,
                end_time,
                timezone,
            } => {
                assert_eq!(start_day, Weekday::Sun);
                assert_eq!(start_time, NaiveTime::from_hms_opt(18, 0, 0).unwrap());
                assert_eq!(end_day, Weekday::Fri);
                assert_eq!(end_time, NaiveTime::from_hms_opt(17, 0, 0).unwrap());
                assert_eq!(timezone, Tz::UTC);
            }
            _ => panic!("Expected Weekly schedule"),
        }
    }

    #[test]
    fn test_into_weekly_session_with_equal_times_is_still_weekly() {
        let time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let config = ScheduleConfig {
            start_time: Some(time),
            end_time: Some(time),
            start_day: Some(Weekday::Mon),
            end_day: Some(Weekday::Fri),
            weekdays: vec![],
            timezone: None,
        };

        let schedule = SessionSchedule::try_from(config).unwrap();
        assert!(matches!(schedule, SessionSchedule::Weekly { .. }));
    }

    #[test]
    fn test_into_invalid_weekly_with_weekdays() {
        let config = ScheduleConfig {
            start_time: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            end_time: Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            start_day: Some(Weekday::Mon),
            end_day: Some(Weekday::Fri),
            weekdays: vec![Weekday::Mon],
            timezone: None,
        };

        let result = SessionSchedule::try_from(config);
        assert!(result.is_err());
        match result.unwrap_err() {
            SessionError::InvalidSchedule(msg) => {
                assert!(msg.contains("Weekly sessions cannot have weekdays specified"));
            }
        }
    }

    #[test]
    fn test_into_invalid_partial_config_start_time_only() {
        let config = ScheduleConfig {
            start_time: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            end_time: None,
            start_day: None,
            end_day: None,
            weekdays: vec![],
            timezone: None,
        };

        let result = SessionSchedule::try_from(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_into_invalid_partial_config_end_time_only() {
        let config = ScheduleConfig {
            start_time: None,
            end_time: Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            start_day: None,
            end_day: None,
            weekdays: vec![],
            timezone: None,
        };

        let result = SessionSchedule::try_from(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_into_invalid_partial_config_start_day_only() {
        let config = ScheduleConfig {
            start_time: None,
            end_time: None,
            start_day: Some(Weekday::Mon),
            end_day: None,
            weekdays: vec![],
            timezone: None,
        };

        let result = SessionSchedule::try_from(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_into_invalid_mixed_config() {
        let config = ScheduleConfig {
            start_time: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            end_time: None,
            start_day: Some(Weekday::Mon),
            end_day: None,
            weekdays: vec![],
            timezone: None,
        };

        let result = SessionSchedule::try_from(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_into_weekdays_with_single_day() {
        let config = ScheduleConfig {
            start_time: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            end_time: Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            start_day: None,
            end_day: None,
            weekdays: vec![Weekday::Sat],
            timezone: None,
        };

        let schedule = SessionSchedule::try_from(config).unwrap();
        match schedule {
            SessionSchedule::Weekdays { weekdays, .. } => {
                assert_eq!(weekdays, vec![Weekday::Sat]);
            }
            _ => panic!("Expected Weekdays schedule"),
        }
    }

    #[test]
    fn test_into_weekdays_with_equal_times_is_invalid() {
        let time = NaiveTime::from_hms_opt(10, 30, 0).unwrap();
        let config = ScheduleConfig {
            start_time: Some(time),
            end_time: Some(time),
            start_day: None,
            end_day: None,
            weekdays: vec![Weekday::Mon, Weekday::Wed, Weekday::Fri],
            timezone: None,
        };

        let schedule = SessionSchedule::try_from(config);
        assert!(schedule.is_err());
    }
}
