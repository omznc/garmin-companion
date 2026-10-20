//! The screens of the app and the groups that the sidebar sorts them into.

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
}
