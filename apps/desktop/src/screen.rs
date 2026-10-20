//! The screens of the app and the groups that the sidebar sorts them into.

use chrono::{Datelike, Duration, NaiveDate};

use crate::icons::Icon;

/// A screen. The variants are in the default sidebar order, group by group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Today,
    Health,
    Sleep,
    Food,
    Weight,
    Activities,
    Strength,
    Fitness,
    Plan,
    Routes,
    Gear,
    Ask,
    Insights,
    Reports,
    Settings,
}

/// A sidebar group. A screen stays in its group when the user reorders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Daily,
    Training,
    Coach,
    /// Settings stands alone at the bottom.
    Settings,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Daily, Group::Training, Group::Coach, Group::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Group::Daily => "Daily",
            Group::Training => "Training",
            Group::Coach => "Coach",
            Group::Settings => "Settings",
        }
    }

    /// The saved name of the group: the lowercase label.
    pub fn key(self) -> String {
        self.label().to_lowercase()
    }
}

impl Screen {
    pub const ALL: [Screen; 15] = [
        Screen::Today,
        Screen::Health,
        Screen::Sleep,
        Screen::Food,
        Screen::Weight,
        Screen::Activities,
        Screen::Strength,
        Screen::Fitness,
        Screen::Plan,
        Screen::Routes,
        Screen::Gear,
        Screen::Ask,
        Screen::Insights,
        Screen::Reports,
        Screen::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Screen::Today => "Today",
            Screen::Health => "Health",
            Screen::Sleep => "Sleep",
            Screen::Food => "Food",
            Screen::Weight => "Weight",
            Screen::Activities => "Activities",
            Screen::Strength => "Strength",
            Screen::Fitness => "Fitness",
            Screen::Plan => "Plan",
            Screen::Routes => "Routes",
            Screen::Gear => "Gear",
            Screen::Ask => "Ask",
            Screen::Insights => "Insights",
            Screen::Reports => "Reports",
            Screen::Settings => "Settings",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Screen::Today => Icon::NavToday,
            Screen::Health => Icon::NavHealth,
            Screen::Sleep => Icon::NavSleep,
            Screen::Food => Icon::NavFood,
            Screen::Weight => Icon::NavWeight,
            Screen::Activities => Icon::NavActivities,
            Screen::Strength => Icon::NavStrength,
            Screen::Fitness => Icon::NavFitness,
            Screen::Plan => Icon::NavPlan,
            Screen::Routes => Icon::NavRoutes,
            Screen::Gear => Icon::NavGear,
            Screen::Ask => Icon::NavAsk,
            Screen::Insights => Icon::NavInsights,
            Screen::Reports => Icon::NavReports,
            Screen::Settings => Icon::NavSettings,
        }
    }

    pub fn group(self) -> Group {
        match self {
            Screen::Today | Screen::Health | Screen::Sleep | Screen::Food | Screen::Weight => {
                Group::Daily
            }
            Screen::Activities
            | Screen::Strength
            | Screen::Fitness
            | Screen::Plan
            | Screen::Routes
            | Screen::Gear => Group::Training,
            Screen::Ask | Screen::Insights | Screen::Reports => Group::Coach,
            Screen::Settings => Group::Settings,
        }
    }

    /// The saved name of the screen: the lowercase label. It never changes.
    pub fn key(self) -> String {
        self.label().to_lowercase()
    }

    pub fn from_key(key: &str) -> Option<Screen> {
        Screen::ALL.into_iter().find(|s| s.key() == key)
    }

    /// The screen with this label, in any case. For the command line.
    pub fn from_label(label: &str) -> Option<Screen> {
        Screen::ALL
            .into_iter()
            .find(|s| s.label().eq_ignore_ascii_case(label))
    }

    /// The page under the screen name in the breadcrumb bar. `day` is the
    /// date of the screen, so a sentence with a date names a real day.
    pub fn crumb(self, day: NaiveDate) -> String {
        let long = |d: NaiveDate| d.format("%-d %B").to_string();
        match self {
            Screen::Today | Screen::Food => day.format("%A, %-d %B").to_string(),
            Screen::Sleep => format!("Night of {}", long(day - Duration::days(1))),
            Screen::Plan => {
                let monday = day - Duration::days(i64::from(day.weekday().num_days_from_monday()));
                format!("Week of {}", long(monday))
            }
            Screen::Activities => "All sports".into(),
            Screen::Health | Screen::Strength => "Last 30 days".into(),
            Screen::Weight => "Last 90 days".into(),
            Screen::Fitness => "Today".into(),
            Screen::Routes => "Saved routes".into(),
            Screen::Gear => "Shoes and devices".into(),
            Screen::Ask => "Conversation".into(),
            Screen::Insights => "New this week".into(),
            Screen::Reports => "All reports".into(),
            Screen::Settings => "All settings".into(),
        }
    }

    /// The place of the screen in `ALL`.
    pub fn index(self) -> usize {
        self as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variants_are_in_the_order_of_all_and_the_groups_are_whole() {
        for (i, s) in Screen::ALL.into_iter().enumerate() {
            assert_eq!(s.index(), i);
            assert_eq!(Screen::from_key(&s.key()), Some(s));
        }
        // A group is one run, in the order of `Group::ALL`.
        let groups: Vec<Group> = Screen::ALL.iter().map(|s| s.group()).collect();
        let mut runs = groups.clone();
        runs.dedup();
        assert_eq!(runs, Group::ALL);
    }

    #[test]
    fn a_crumb_names_a_real_day() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        assert_eq!(Screen::Today.crumb(day), "Tuesday, 20 October");
        assert_eq!(Screen::Sleep.crumb(day), "Night of 19 October");
        assert_eq!(Screen::Plan.crumb(day), "Week of 19 October");
    }
}
