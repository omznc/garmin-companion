//! The actions that the command palette lists, and the filter over them.

use crate::fuzzy;
use crate::screen::Screen;

/// What the shell does when the user picks a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Show this screen.
    Go(Screen),
    SyncNow,
    CheckUpdates,
    /// Show the Ask screen with an empty conversation.
    NewChat,
    ToggleTheme,
}

pub struct Command {
    pub label: String,
    pub action: Action,
}

fn command(label: impl Into<String>, action: Action) -> Command {
    Command {
        label: label.into(),
        action,
    }
}

/// Every command, in the order that the empty query shows.
pub fn all() -> Vec<Command> {
    let mut list = vec![
        command("Sync now", Action::SyncNow),
        command("New chat", Action::NewChat),
        command("Check for updates", Action::CheckUpdates),
        command("Toggle light and dark", Action::ToggleTheme),
    ];
    list.extend(Screen::ALL.into_iter().map(|screen| {
        let verb = if screen == Screen::Settings {
            "Open"
        } else {
            "Go to"
        };
        command(format!("{verb} {}", screen.label()), Action::Go(screen))
    }));
    list
}

/// The indices of the commands that match `query`, best first. An empty query
/// keeps all of them in their order.
pub fn filter(commands: &[Command], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let mut hits: Vec<(usize, i32)> = commands
        .iter()
        .enumerate()
        .filter_map(|(i, c)| Some((i, fuzzy::subsequence(&query, &c.label.to_lowercase())?)))
        .collect();
    // A stable sort keeps the list order for equal scores.
    hits.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
    hits.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(query: &str) -> Vec<String> {
        let list = all();
        filter(&list, query)
            .into_iter()
            .map(|i| list[i].label.clone())
            .collect()
    }

    #[test]
    fn an_empty_query_lists_everything_in_order() {
        assert_eq!(labels("  ").len(), all().len());
        assert_eq!(labels("")[0], "Sync now");
    }

    #[test]
    fn a_screen_has_one_command() {
        let list = all();
        for screen in Screen::ALL {
            let hits = list.iter().filter(|c| c.action == Action::Go(screen));
            assert_eq!(hits.count(), 1);
        }
    }

    #[test]
    fn the_filter_finds_a_command_from_a_few_letters() {
        assert_eq!(labels("snc")[0], "Sync now");
        assert_eq!(labels("SLEEP"), ["Go to Sleep"]);
        assert_eq!(labels("settings"), ["Open Settings"]);
    }

    #[test]
    fn the_filter_drops_what_does_not_match() {
        assert!(labels("zzzz").is_empty());
    }

    #[test]
    fn the_better_match_comes_first() {
        // "Go to Health" has the word start `h`, "Check for updates" only a buried one.
        assert_eq!(labels("health")[0], "Go to Health");
        let hits = labels("up");
        assert_eq!(hits[0], "Check for updates");
    }
}
