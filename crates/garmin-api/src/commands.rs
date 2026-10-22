//! The list of commands that `call` runs, with their TypeScript types.
//!
//! The mobile app reads this list through a generated `Commands` map, so a
//! call with a wrong name, wrong arguments or a wrong result type fails the
//! TypeScript check. The strings are TypeScript type names that the generated
//! files export. `undefined` as `args` means the command takes none.
//! A command whose arguments are all optional lists `| undefined`.

/// One command of the JSON bridge.
#[derive(Debug, Clone, Copy)]
pub struct Command {
    pub name: &'static str,
    /// The TypeScript type of the arguments.
    pub args: &'static str,
    /// The TypeScript type of the result.
    pub result: &'static str,
}

const fn cmd(name: &'static str, args: &'static str, result: &'static str) -> Command {
    Command { name, args, result }
}

/// Every command of [`crate::call`]. The order is the order of the generated map.
pub const COMMANDS: &[Command] = &[
    cmd("today", "TodayArgs | undefined", "TodayView"),
    cmd(
        "cached_daily",
        "DaysArgs | undefined",
        "Array<DailyMetrics>",
    ),
    cmd(
        "cached_activities",
        "RecentArgs | undefined",
        "Array<CachedActivity>",
    ),
    cmd(
        "cached_activities_since",
        "SinceArgs",
        "Array<CachedActivity>",
    ),
    cmd("cached_activity", "ActivityIdArgs", "CachedActivity | null"),
    cmd("nutrition", "DaysArgs | undefined", "NutritionReport"),
    cmd("weight", "DaysArgs | undefined", "WeightReport"),
    cmd("activities", "RecentArgs | undefined", "ActivitiesView"),
    cmd(
        "activity_detail",
        "ActivityIdArgs",
        "ActivityDetailView | null",
    ),
    cmd("health_view", "undefined", "HealthView"),
    cmd("sleep_view", "undefined", "SleepView"),
    cmd("weight_view", "undefined", "WeightView"),
    cmd("food_view", "undefined", "FoodView"),
    cmd("strength_view", "undefined", "StrengthView"),
    cmd("plan_view", "undefined", "PlanView"),
    cmd("routes_view", "RoutesArgs | undefined", "RoutesView"),
    cmd("gear_view", "undefined", "GearView"),
    cmd("insights_view", "undefined", "InsightsView"),
    cmd("fitness_view", "undefined", "FitnessView"),
    cmd("report_view", "OffsetArgs | undefined", "ReportView"),
    cmd("tiles_view", "undefined", "TilesView"),
    cmd("extras_view", "undefined", "ExtrasView"),
    cmd("charts_view", "undefined", "ChartsView"),
    cmd("status", "undefined", "Status"),
    cmd("sign_in_url", "undefined", "string"),
    cmd("redeem_ticket", "TicketArgs", "Status"),
    cmd("import_tokens", "ImportArgs | undefined", "Status"),
    cmd("sign_out", "undefined", "Status"),
    cmd("settings", "undefined", "SettingsView"),
    cmd("set_theme", "ThemeArgs", "Prefs"),
    cmd("set_nav_order", "NavOrderArgs", "Prefs"),
    cmd("set_palettes", "PalettesArgs", "Prefs"),
];

/// The TypeScript text of a command map: `export type Commands = { ... }`.
pub fn commands_type(commands: &[Command]) -> String {
    let mut out = String::from("export type Commands = {\n");
    for c in commands {
        out.push_str(&format!(
            "  {}: {{ args: {}; result: {} }};\n",
            c.name, c.args, c.result
        ));
    }
    out.push_str("};\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The names that the dispatch `match` arms handle, read from its source.
    fn dispatched() -> HashSet<String> {
        let src = include_str!("dispatch.rs");
        let body = src.split("#[cfg(test)]").next().unwrap();
        body.lines()
            .map(str::trim_start)
            .filter(|l| l.starts_with('"') && (l.contains("=>") || l.ends_with('|')))
            .flat_map(|l| {
                l.split("=>")
                    .next()
                    .unwrap()
                    .split('|')
                    .filter_map(|n| n.trim().strip_prefix('"')?.strip_suffix('"'))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn the_list_and_the_dispatch_match_agree() {
        let listed: HashSet<String> = COMMANDS.iter().map(|c| c.name.to_owned()).collect();
        assert_eq!(listed.len(), COMMANDS.len(), "a command is listed twice");
        assert_eq!(listed, dispatched());
    }

    /// Functions that the desktop may call and that have no command, and the
    /// reason. A new `*_view` function must get a command or a line here.
    /// The test fails for a view that has neither.
    const NO_COMMAND: &[(&str, &str)] = &[
        // Fixture builders. They take a persona or a clock, so a person never asks for them.
        ("body_views", "builds the fixture file"),
        ("places_views", "builds the fixture file"),
        ("training_views", "builds the fixture file"),
        ("insights_view_seen", "fixture builder with a fixed clock"),
        ("activities_view_rows", "a helper of activities_view"),
        // A binary result: a JSON bridge has no use for it. The desktop saves it to a file.
        ("weekly_report_pdf", "returns PDF bytes"),
        ("monthly_report_pdf", "returns PDF bytes"),
        (
            "activities_csv",
            "returns a CSV file for the desktop save dialog",
        ),
    ];

    /// Functions that are not views, but the desktop calls them, so the mobile app needs them too.
    const DESKTOP_ACTIONS: &[&str] = &[
        "set_theme",
        "set_nav_order",
        "set_palettes",
        "sign_out",
        "redeem_ticket",
        "import_tokens",
        "sign_in_url",
        "status",
    ];

    /// The command that a view function maps to: `today_view` runs `today`.
    fn command_for(function: &str) -> Vec<String> {
        vec![
            function.to_owned(),
            function.trim_end_matches("_view").to_owned(),
        ]
    }

    #[test]
    fn every_public_view_has_a_command_or_a_reason() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let listed: HashSet<&str> = COMMANDS.iter().map(|c| c.name).collect();
        let allowed: HashSet<&str> = NO_COMMAND.iter().map(|(f, _)| *f).collect();
        let mut missing = Vec::new();
        let mut found = HashSet::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(path).unwrap();
            let body = text.split("#[cfg(test)]").next().unwrap();
            for line in body.lines() {
                let Some(rest) = line.strip_prefix("pub fn ") else {
                    continue;
                };
                let name = rest.split(['(', '<']).next().unwrap();
                found.insert(name.to_owned());
                let wanted = name.ends_with("_view")
                    || name.ends_with("_views")
                    || DESKTOP_ACTIONS.contains(&name);
                let has_command = command_for(name)
                    .iter()
                    .any(|n| listed.contains(n.as_str()));
                if wanted && !has_command && !allowed.contains(name) {
                    missing.push(name.to_owned());
                }
            }
        }
        assert!(missing.is_empty(), "no command for: {missing:?}");
        // The scan must see the code, and a line of the list must name a real function.
        assert!(found.contains("today_view"));
        for name in &allowed {
            assert!(
                found.contains(*name),
                "{name} is in the list but not in the code"
            );
        }
    }

    #[test]
    fn the_commands_type_lists_each_command() {
        let text = commands_type(COMMANDS);
        assert!(text.contains("today: { args: TodayArgs | undefined; result: TodayView };"));
        assert_eq!(text.matches("result:").count(), COMMANDS.len());
    }
}
