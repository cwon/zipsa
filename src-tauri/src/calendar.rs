use chrono::{DateTime, Duration, Utc};

use crate::{
    models::{CalendarEvent, Snapshot, Task},
    schedule::{next_due, parse_time},
    store::space_path,
};

const MAX_EVENTS: usize = 20_000;

fn target(data: &Snapshot, task: &Task) -> String {
    if let Some(id) = &task.space_id {
        return space_path(data, id);
    }
    data.items
        .iter()
        .find(|item| Some(&item.id) == task.item_id.as_ref())
        .map(|item| format!("{} / {}", space_path(data, &item.space_id), item.name))
        .unwrap_or_default()
}

/// The range is half-open and expressed as UTC instants. The frontend sends
/// local midnight boundaries, so events at midnight and DST transitions belong
/// to the same dates as the rest of the app. Projection never modifies tasks.
pub fn events(data: &Snapshot, start: &str, end: &str) -> Result<Vec<CalendarEvent>, String> {
    let start = parse_time(start)?;
    let end = parse_time(end)?;
    if end <= start || end - start > Duration::days(63) {
        return Err("달력 조회 기간이 올바르지 않습니다. 최대 63일까지 조회할 수 있어요.".into());
    }
    let mut result: Vec<(DateTime<Utc>, CalendarEvent)> = vec![];
    for record in &data.records {
        let at = parse_time(&record.scheduled_for)?;
        if at >= start && at < end {
            result.push((
                at,
                CalendarEvent {
                    id: format!("record:{}", record.id),
                    task_id: record.task_id.clone(),
                    title: record.title.clone(),
                    target: record.target.clone(),
                    scheduled_for: record.scheduled_for.clone(),
                    kind: record.outcome.clone(),
                    record_id: Some(record.id.clone()),
                    completed_at: Some(record.completed_at.clone()),
                },
            ));
            if result.len() > MAX_EVENTS {
                return Err("일정이 너무 많아 달력에 모두 표시할 수 없어요.".into());
            }
        }
    }
    for task in data.tasks.iter().filter(|task| task.active) {
        let due = parse_time(&task.due_at)?;
        if due >= end {
            continue;
        }
        let label = target(data, task);
        let event = |at: DateTime<Utc>, projected: bool| CalendarEvent {
            id: format!("task:{}:{}", task.id, at.to_rfc3339()),
            task_id: task.id.clone(),
            title: task.title.clone(),
            target: label.clone(),
            scheduled_for: at.to_rfc3339(),
            kind: if projected { "projected" } else { "pending" }.into(),
            record_id: None,
            completed_at: None,
        };
        if due >= start {
            result.push((due, event(due, false)));
        }
        // Completion-based dates are unknown until the current occurrence is
        // processed. Only calendar-based rules have predictable future dates.
        if task.mode == "calendar" {
            let before_start = start
                .checked_sub_signed(Duration::nanoseconds(1))
                .ok_or("달력 날짜가 지원하는 범위를 벗어났습니다.")?;
            let mut at =
                next_due(task, due.max(before_start))?.ok_or("반복 일정을 계산하지 못했습니다.")?;
            loop {
                let instant = parse_time(&at)?;
                if instant >= end {
                    break;
                }
                result.push((instant, event(instant, true)));
                if result.len() > MAX_EVENTS {
                    return Err("일정이 너무 많아 달력에 모두 표시할 수 없어요.".into());
                }
                at = next_due(task, instant)?.ok_or("반복 일정을 계산하지 못했습니다.")?;
            }
        }
        if result.len() > MAX_EVENTS {
            return Err("일정이 너무 많아 달력에 모두 표시할 수 없어요.".into());
        }
    }
    result.sort_by(|(at, a), (bt, b)| at.cmp(bt).then(a.id.cmp(&b.id)));
    Ok(result.into_iter().map(|(_, event)| event).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Record, Space};

    fn data(task: Task) -> Snapshot {
        Snapshot {
            spaces: vec![Space {
                id: "home".into(),
                parent_id: None,
                name: "우리 집".into(),
                icon: "🏠".into(),
                notes: "".into(),
            }],
            tasks: vec![task],
            ..Snapshot::default()
        }
    }

    #[test]
    fn projects_month_ends_without_advancing_the_stored_occurrence() {
        let original = crate::schedule::tests::task();
        let data = data(original.clone());
        let events = events(&data, "2026-02-01T00:00:00Z", "2026-04-01T00:00:00Z").unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].scheduled_for, "2026-02-28T00:00:00+00:00");
        assert_eq!(events[1].scheduled_for, "2026-03-31T00:00:00+00:00");
        assert!(events.iter().all(|event| event.kind == "projected"));
        assert_eq!(data.tasks[0].due_at, original.due_at);
        assert_eq!(events[0].target, "우리 집");
    }

    #[test]
    fn respects_local_midnight_boundaries_and_daylight_saving() {
        let mut task = crate::schedule::tests::task();
        task.unit = "day".into();
        task.timezone = "America/New_York".into();
        task.anchor_at = "2026-03-07T14:00:00Z".into();
        task.due_at = task.anchor_at.clone();
        let events = events(&data(task), "2026-03-07T05:00:00Z", "2026-03-10T04:00:00Z").unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, "pending");
        assert_eq!(
            parse_time(&events[1].scheduled_for).unwrap(),
            parse_time("2026-03-08T13:00:00Z").unwrap()
        );
        assert_eq!(
            parse_time(&events[2].scheduled_for).unwrap(),
            parse_time("2026-03-09T13:00:00Z").unwrap()
        );
    }

    #[test]
    fn only_projects_predictable_dates_and_excludes_inactive_tasks() {
        let mut task = crate::schedule::tests::task();
        for mode in ["once", "after_completion"] {
            task.mode = mode.into();
            let result = events(
                &data(task.clone()),
                "2026-01-01T00:00:00Z",
                "2026-03-01T00:00:00Z",
            )
            .unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].kind, "pending");
        }
        task.active = false;
        assert!(
            events(&data(task), "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn retains_deleted_task_history_on_the_scheduled_date_with_exclusive_end() {
        let mut data = Snapshot::default();
        data.records.push(Record {
            id: "record".into(),
            task_id: "deleted".into(),
            title: "지난 청소".into(),
            target: "우리 집 / 책장".into(),
            scheduled_for: "2026-11-08T00:00:00+09:00".into(),
            completed_at: "2026-11-10T09:00:00+09:00".into(),
            outcome: "completed".into(),
            task_after: None,
        });
        let result = events(&data, "2026-11-07T15:00:00Z", "2026-11-08T15:00:00Z").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, "completed");
        assert_eq!(result[0].record_id.as_deref(), Some("record"));
        assert!(
            events(&data, "2026-11-06T15:00:00Z", "2026-11-07T15:00:00Z")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn validates_ranges_and_can_jump_over_years_of_unprocessed_recurrences() {
        let mut task = crate::schedule::tests::task();
        task.unit = "day".into();
        let data = data(task);
        assert!(events(&data, "bad", "2026-02-01T00:00:00Z").is_err());
        assert!(events(&data, "2026-02-01T00:00:00Z", "2026-02-01T00:00:00Z").is_err());
        assert!(events(&data, "2026-01-01T00:00:00Z", "2027-01-01T00:00:00Z").is_err());
        let result = events(&data, "2036-01-01T00:00:00Z", "2036-01-03T00:00:00Z").unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|event| event.kind == "projected"));
    }
}
