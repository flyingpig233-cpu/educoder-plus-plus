use educoder_core::models::Game;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct HomeworkProgress {
    games: HashMap<String, Option<bool>>,
    total: Option<usize>,
    unstarted: bool,
}

impl HomeworkProgress {
    pub fn set_games(&mut self, games: &[Game], force: bool) {
        self.games = games
            .iter()
            .map(|game| {
                let status = if force {
                    None
                } else {
                    self.games.get(&game.identifier).copied().flatten()
                };
                (game.identifier.clone(), status)
            })
            .collect();
        self.total = Some(self.games.len());
        self.unstarted = false;
    }

    pub fn set_unstarted(&mut self, total: usize, confirmed: bool) {
        self.games.clear();
        self.total = Some(total);
        self.unstarted = confirmed;
    }

    pub fn update(&mut self, game: &str, status: i32) {
        if let Some(value) = self.games.get_mut(game) {
            *value = Some(status == 2);
        }
    }

    pub fn display(&self) -> (String, bool) {
        let Some(total) = self.total.filter(|total| *total > 0) else {
            return ("— / —".into(), false);
        };
        if self.unstarted {
            return (format!("0 / {total}"), false);
        }
        if self.games.len() != total || self.games.values().any(Option::is_none) {
            return (format!("— / {total}"), false);
        }
        let passed = self
            .games
            .values()
            .filter(|value| **value == Some(true))
            .count();
        (format!("{passed} / {total}"), passed == total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_requires_every_unique_game_to_be_confirmed_passed() {
        let mut progress = HomeworkProgress::default();
        let games = ["a", "b", "a"].map(|id| Game {
            identifier: id.into(),
            position: 1,
            name: id.into(),
        });
        progress.set_games(&games, false);
        progress.update("a", 2);
        assert_eq!(progress.display(), ("— / 2".into(), false));
        progress.update("b", 3);
        assert_eq!(progress.display(), ("1 / 2".into(), false));
        progress.update("b", 2);
        assert_eq!(progress.display(), ("2 / 2".into(), true));
        progress.update("b", 1);
        assert_eq!(progress.display(), ("1 / 2".into(), false));
        progress.set_games(&games, true);
        assert_eq!(progress.display(), ("— / 2".into(), false));
    }

    #[test]
    fn an_empty_or_unknown_instance_is_never_complete() {
        let mut progress = HomeworkProgress::default();
        assert_eq!(progress.display(), ("— / —".into(), false));
        progress.set_unstarted(4, false);
        assert_eq!(progress.display(), ("— / 4".into(), false));
        progress.set_unstarted(4, true);
        assert_eq!(progress.display(), ("0 / 4".into(), false));
        progress.set_unstarted(0, true);
        assert!(!progress.display().1);
    }
}
