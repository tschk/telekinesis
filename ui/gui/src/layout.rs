use crate::session::SessionKind;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionRow {
    pub index: usize,
    pub name: String,
    pub kind: SessionKind,
    pub active: bool,
    pub busy: bool,
    pub is_computer: bool,
}

pub fn next_effort(current: &str) -> String {
    match current {
        "low" => "medium",
        "medium" => "high",
        "high" => "xhigh",
        _ => "low",
    }
    .to_string()
}

pub fn composer_action(busy: bool, connected: bool) -> &'static str {
    if busy {
        "queue"
    } else if connected {
        "send"
    } else {
        "login"
    }
}

pub fn session_rows(names: &[(String, SessionKind, bool)], active: usize) -> Vec<SessionRow> {
    names
        .iter()
        .enumerate()
        .map(|(index, (name, kind, busy))| SessionRow {
            index,
            name: name.clone(),
            kind: *kind,
            active: index == active,
            busy: *busy,
            is_computer: *kind == SessionKind::ComputerUse,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_cycles_low_to_xhigh() {
        assert_eq!(next_effort("low"), "medium");
        assert_eq!(next_effort("medium"), "high");
        assert_eq!(next_effort("high"), "xhigh");
        assert_eq!(next_effort("xhigh"), "low");
        assert_eq!(next_effort(""), "low");
    }

    #[test]
    fn composer_queues_while_busy() {
        assert_eq!(composer_action(true, true), "queue");
        assert_eq!(composer_action(false, true), "send");
        assert_eq!(composer_action(false, false), "login");
    }

    #[test]
    fn session_list_marks_active_row() {
        let rows = session_rows(
            &[
                ("computer use".into(), SessionKind::ComputerUse, false),
                ("coding".into(), SessionKind::Coding, true),
            ],
            1,
        );
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].active);
        assert!(rows[1].active);
        assert!(rows[1].busy);
        assert!(rows[0].is_computer);
        assert!(!rows[1].is_computer);
    }
}
