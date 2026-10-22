//! The Garmin food log: meals and the goals of a day.
//!
//! The account that was probed logs food in another app, so the shape of a
//! meal is not confirmed. The parser reads each field under the names that
//! the nutrition service is known or likely to use, and every field is
//! optional. One odd meal never drops the others.

use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;

pub(crate) const SCHEMA: &str = r#"
    CREATE TABLE IF NOT EXISTS nutrition_meals (
        date      TEXT NOT NULL,
        idx       INTEGER NOT NULL,
        name      TEXT,
        time      TEXT,
        kcal      REAL,
        protein_g REAL,
        carbs_g   REAL,
        fat_g     REAL,
        PRIMARY KEY (date, idx)
    );

    CREATE TABLE IF NOT EXISTS nutrition_goals (
        date              TEXT PRIMARY KEY,
        calories          REAL,
        adjusted_calories REAL,
        protein_g         REAL,
        carbs_g           REAL,
        fat_g             REAL
    );
"#;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Meal {
    pub name: Option<String>,
    /// Time of day as Garmin sends it.
    pub time: Option<String>,
    pub kcal: Option<f64>,
    pub protein_g: Option<f64>,
    pub carbs_g: Option<f64>,
    pub fat_g: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct NutritionGoals {
    pub calories: Option<f64>,
    /// The goal after Garmin adds the calories of the day's activity.
    pub adjusted_calories: Option<f64>,
    pub protein_g: Option<f64>,
    pub carbs_g: Option<f64>,
    pub fat_g: Option<f64>,
}

/// The first of `keys` that holds a number, or a number written as text.
fn first_num(v: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|k| {
        let x = v.get(*k)?;
        x.as_f64().or_else(|| x.as_str()?.trim().parse().ok())
    })
}

fn first_text(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(*k)?.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

const KCAL: &[&str] = &["calories", "kcal", "energy"];
const PROTEIN: &[&str] = &["protein", "proteinGrams", "proteinInGrams"];
const CARBS: &[&str] = &["carbs", "carbohydrates", "carbohydrate", "carbsGrams"];
const FAT: &[&str] = &["fat", "totalFat", "fatGrams"];

/// The goals in a food log reply. `None` when the reply has no goal object or
/// the object holds no figure.
pub fn parse_goals(v: &Value) -> Option<NutritionGoals> {
    let g = v.get("dailyNutritionGoals")?;
    let goals = NutritionGoals {
        calories: first_num(g, KCAL),
        adjusted_calories: first_num(g, &["adjustedCalories"]),
        protein_g: first_num(g, PROTEIN),
        carbs_g: first_num(g, CARBS),
        fat_g: first_num(g, FAT),
    };
    (goals != NutritionGoals::default()).then_some(goals)
}

/// One meal-like object. The figures sit in a nested object in some replies
/// and on the meal itself in others.
fn meal_of(m: &Value) -> Option<Meal> {
    if !m.is_object() {
        return None;
    }
    let n = ["nutritionContents", "nutrition", "nutritionalContents"]
        .iter()
        .find_map(|k| m.get(*k).filter(|x| x.is_object()))
        .unwrap_or(m);
    let meal = Meal {
        name: first_text(m, &["mealName", "name", "mealType", "foodName"]).or_else(|| {
            m.pointer("/foodMetaData/foodName")?
                .as_str()
                .map(str::to_owned)
        }),
        time: first_text(
            m,
            &[
                "mealTime",
                "time",
                "startTime",
                "loggedTime",
                "mealStartTime",
            ],
        ),
        kcal: first_num(n, KCAL),
        protein_g: first_num(n, PROTEIN),
        carbs_g: first_num(n, CARBS),
        fat_g: first_num(n, FAT),
    };
    (meal != Meal::default()).then_some(meal)
}

/// The meals in a food log reply. The meal list is used when it has entries.
/// The logged foods are the fallback, one entry for each food.
pub fn parse_meals(v: &Value) -> Vec<Meal> {
    let list = |k: &str| {
        v.get(k)
            .and_then(Value::as_array)
            .map(|l| l.iter().filter_map(meal_of).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let meals = list("mealDetails");
    if meals.is_empty() {
        list("loggedFoodsWithServingSizes")
    } else {
        meals
    }
}

impl Db {
    /// Replace the meals and the goals of one day. A day with neither leaves
    /// no row.
    pub fn save_nutrition_day(
        &self,
        date: &str,
        goals: Option<&NutritionGoals>,
        meals: &[Meal],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM nutrition_meals WHERE date = ?1", params![date])?;
        tx.execute("DELETE FROM nutrition_goals WHERE date = ?1", params![date])?;
        for (i, m) in meals.iter().enumerate() {
            tx.execute(
                "INSERT INTO nutrition_meals (date, idx, name, time, kcal, protein_g, carbs_g, fat_g)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![date, i as i64, m.name, m.time, m.kcal, m.protein_g, m.carbs_g, m.fat_g],
            )?;
        }
        if let Some(g) = goals {
            tx.execute(
                "INSERT INTO nutrition_goals (date, calories, adjusted_calories, protein_g, carbs_g, fat_g)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![date, g.calories, g.adjusted_calories, g.protein_g, g.carbs_g, g.fat_g],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The meals of one day, in the order that Garmin listed them.
    pub fn nutrition_meals(&self, date: &str) -> Result<Vec<Meal>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, time, kcal, protein_g, carbs_g, fat_g FROM nutrition_meals
             WHERE date = ?1 ORDER BY idx",
        )?;
        let rows = stmt
            .query_map(params![date], |r| {
                Ok(Meal {
                    name: r.get(0)?,
                    time: r.get(1)?,
                    kcal: r.get(2)?,
                    protein_g: r.get(3)?,
                    carbs_g: r.get(4)?,
                    fat_g: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// The goals of one day, if the cache holds them.
    pub fn nutrition_goals(&self, date: &str) -> Result<Option<NutritionGoals>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row(
                "SELECT calories, adjusted_calories, protein_g, carbs_g, fat_g
                 FROM nutrition_goals WHERE date = ?1",
                params![date],
                |r| {
                    Ok(NutritionGoals {
                        calories: r.get(0)?,
                        adjusted_calories: r.get(1)?,
                        protein_g: r.get(2)?,
                        carbs_g: r.get(3)?,
                        fat_g: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn goals_read_the_calories_and_any_macro_under_its_likely_name() {
        let v = json!({ "dailyNutritionGoals": { "calories": 2000, "adjustedCalories": 2200, "protein": "120", "carbohydrates": 250 } });
        let g = parse_goals(&v).unwrap();
        assert_eq!(
            (g.calories, g.adjusted_calories),
            (Some(2000.0), Some(2200.0))
        );
        assert_eq!(
            (g.protein_g, g.carbs_g, g.fat_g),
            (Some(120.0), Some(250.0), None)
        );
        assert!(parse_goals(&json!({ "dailyNutritionGoals": {} })).is_none());
        assert!(parse_goals(&json!({})).is_none());
    }

    #[test]
    fn a_meal_reads_nested_or_flat_figures_and_an_odd_one_is_skipped() {
        let v = json!({ "mealDetails": [
            { "mealName": "Lunch", "mealTime": "12:30:00",
              "nutritionContents": { "calories": 600, "protein": 40, "carbs": 70, "fat": 15 } },
            { "name": "Snack", "calories": 150 },
            "junk",
            {},
            { "mealName": "Empty" },
        ]});
        let got = parse_meals(&v);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].fat_g, Some(15.0));
        assert_eq!(
            (got[1].name.as_deref(), got[1].kcal, got[1].protein_g),
            (Some("Snack"), Some(150.0), None)
        );
        assert_eq!(got[2].kcal, None);
    }

    #[test]
    fn the_logged_foods_stand_in_when_the_meal_list_is_empty() {
        let v = json!({ "mealDetails": [], "loggedFoodsWithServingSizes": [
            { "foodMetaData": { "foodName": "Apple" }, "nutritionContents": { "calories": 80 } },
        ]});
        let got = parse_meals(&v);
        assert_eq!(
            (got[0].name.as_deref(), got[0].kcal),
            (Some("Apple"), Some(80.0))
        );
        assert!(
            parse_meals(&json!({ "mealDetails": [], "loggedFoodsWithServingSizes": [] }))
                .is_empty()
        );
    }

    #[test]
    fn a_day_is_replaced_whole() {
        let db = crate::db::scratch_db("nutrition");
        let goals = NutritionGoals {
            calories: Some(2000.0),
            ..Default::default()
        };
        let meal = |n: &str| Meal {
            name: Some(n.into()),
            kcal: Some(1.0),
            ..Default::default()
        };
        db.save_nutrition_day("2026-03-10", Some(&goals), &[meal("a"), meal("b")])
            .unwrap();
        assert_eq!(db.nutrition_meals("2026-03-10").unwrap().len(), 2);
        db.save_nutrition_day("2026-03-10", None, &[meal("c")])
            .unwrap();
        let left = db.nutrition_meals("2026-03-10").unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].name.as_deref(), Some("c"));
        assert_eq!(db.nutrition_goals("2026-03-10").unwrap(), None);
    }
}
