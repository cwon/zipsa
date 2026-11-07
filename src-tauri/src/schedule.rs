use chrono::{DateTime, Datelike, Days, Months, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::models::Task;

pub fn parse_time(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| "날짜와 시간이 올바르지 않습니다.".into())
}

// Resolve DST gaps forward and choose the first occurrence of an ambiguous time.
fn resolve(tz: Tz, mut local: NaiveDateTime) -> Result<DateTime<Utc>, String> {
    for _ in 0..=180 {
        if let Some(t) = tz.from_local_datetime(&local).earliest() {
            return Ok(t.with_timezone(&Utc));
        }
        local += chrono::Duration::minutes(1);
    }
    Err("해당 시간대에서 실행 시간을 계산하지 못했습니다.".into())
}

fn shifted(anchor: DateTime<Utc>, tz: Tz, count: u32, unit: &str) -> Result<DateTime<Utc>, String> {
    let local = anchor.with_timezone(&tz).naive_local();
    let date = match unit {
        "day" => local.date().checked_add_days(Days::new(count as u64)),
        "week" => local.date().checked_add_days(Days::new(count as u64 * 7)),
        "month" => local.date().checked_add_months(Months::new(count)),
        "year" => count
            .checked_mul(12)
            .and_then(|n| local.date().checked_add_months(Months::new(n))),
        _ => return Err("지원하지 않는 반복 단위입니다.".into()),
    }
    .ok_or("반복 일정이 지원하는 날짜 범위를 벗어났습니다.")?;
    resolve(tz, date.and_time(local.time()))
}

pub fn next_due(task: &Task, finished: DateTime<Utc>) -> Result<Option<String>, String> {
    if task.mode == "once" {
        return Ok(None);
    }
    let tz: Tz = task
        .timezone
        .parse()
        .map_err(|_| "시간대가 올바르지 않습니다.")?;
    if task.every == 0 {
        return Err("반복 간격은 1 이상이어야 합니다.".into());
    }
    if task.mode == "after_completion" {
        return shifted(finished, tz, task.every, &task.unit).map(|d| Some(d.to_rfc3339()));
    }
    if task.mode != "calendar" {
        return Err("지원하지 않는 일정 방식입니다.".into());
    }
    let anchor = parse_time(&task.anchor_at)?;
    let boundary = finished.max(parse_time(&task.due_at)?);
    let a = anchor.with_timezone(&tz).date_naive();
    let b = boundary.with_timezone(&tz).date_naive();
    let periods = match task.unit.as_str() {
        "day" => (b - a).num_days().max(0) as u32,
        "week" => ((b - a).num_days().max(0) / 7) as u32,
        "month" => ((b.year() - a.year()) * 12 + b.month() as i32 - a.month() as i32).max(0) as u32,
        "year" => (b.year() - a.year()).max(0) as u32,
        _ => return Err("지원하지 않는 반복 단위입니다.".into()),
    };
    let mut index = periods / task.every;
    loop {
        let offset = index
            .checked_mul(task.every)
            .ok_or("반복 간격이 너무 큽니다.")?;
        let candidate = shifted(anchor, tz, offset, &task.unit)?;
        if candidate > boundary {
            return Ok(Some(candidate.to_rfc3339()));
        }
        index = index.checked_add(1).ok_or("반복 간격이 너무 큽니다.")?;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn task() -> Task {
        Task {
            id: "task".into(),
            space_id: Some("home".into()),
            item_id: None,
            title: "청소".into(),
            notes: "".into(),
            mode: "calendar".into(),
            every: 1,
            unit: "month".into(),
            anchor_at: "2026-01-31T00:00:00Z".into(),
            due_at: "2026-01-31T00:00:00Z".into(),
            timezone: "Asia/Seoul".into(),
            active: true,
            notified_due_at: None,
            reminder_minutes: None,
            notified_reminder_at: None,
        }
    }

    #[test]
    fn month_end_keeps_original_day_and_skips_missed_periods() {
        let mut t = task();
        assert_eq!(
            parse_time(
                &next_due(&t, parse_time("2026-01-31T02:00:00Z").unwrap())
                    .unwrap()
                    .unwrap()
            )
            .unwrap(),
            parse_time("2026-02-28T00:00:00Z").unwrap()
        );
        t.due_at = "2026-02-28T00:00:00Z".into();
        assert_eq!(
            parse_time(
                &next_due(&t, parse_time("2026-03-05T00:00:00Z").unwrap())
                    .unwrap()
                    .unwrap()
            )
            .unwrap(),
            parse_time("2026-03-31T00:00:00Z").unwrap()
        );
    }

    #[test]
    fn completion_based_uses_actual_completion() {
        let mut t = task();
        t.mode = "after_completion".into();
        t.unit = "day".into();
        t.every = 90;
        let finished = parse_time("2026-02-05T03:00:00Z").unwrap();
        assert_eq!(
            parse_time(&next_due(&t, finished).unwrap().unwrap()).unwrap(),
            finished + chrono::Duration::days(90)
        );
    }

    #[test]
    fn daily_schedule_preserves_wall_clock_across_dst() {
        let mut t = task();
        t.timezone = "America/New_York".into();
        t.unit = "day".into();
        t.anchor_at = "2026-03-07T14:00:00Z".into();
        t.due_at = t.anchor_at.clone();
        assert_eq!(
            parse_time(
                &next_due(&t, parse_time(&t.due_at).unwrap())
                    .unwrap()
                    .unwrap()
            )
            .unwrap(),
            parse_time("2026-03-08T13:00:00Z").unwrap()
        );
    }

    #[test]
    fn early_completion_does_not_repeat_current_occurrence() {
        let t = task();
        let next = next_due(&t, parse_time("2026-01-20T00:00:00Z").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            parse_time(&next).unwrap(),
            parse_time("2026-02-28T00:00:00Z").unwrap()
        );
    }
}
