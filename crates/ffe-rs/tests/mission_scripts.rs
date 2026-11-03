//! The original's mission scripts (cache v32): every mission struct, state
//! and condition in the player's FIRSTENC.EXE decodes to its end exactly as
//! F31/F41 read it. Local-only: needs `FFE_WORLD_CACHE`.
use ffe_core::script::Image;

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn every_script_decodes_as_the_interpreter_reads_it() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let image = Image::new(cache.asset("base:script/missions").unwrap()).unwrap();
    assert_eq!(image.missions(), ffe_core::script::MISSIONS);
    assert_eq!(image.static_words(), 10);
    let (mut states, mut conditions, mut quirky) = (0, 0, Vec::new());
    for mission in 0..image.missions() {
        let record = image.mission_struct(mission).unwrap();
        image.take_quirks();
        let reachable = image.reachable_states(&record).unwrap();
        if image.take_quirks() > 0 {
            quirky.push(mission);
        }
        states += reachable.len();
        for state in reachable {
            conditions += image
                .conditions(image.state(&record, state).unwrap())
                .unwrap();
        }
    }
    assert_eq!((states, conditions), (333, 665));
    // Mission 83's state 4 compares token 0x2b, which F31 reads with two
    // operands although the script gives it one: the condition ends on a
    // token with no case, and the original never runs its goto.
    assert_eq!(quirky, vec![83]);
    // Mission 0, the one F55 starts on a new game: no parameters, four
    // locals and a single state.
    let first = image.mission_struct(0).unwrap();
    assert_eq!((first.parameter_count, first.locals), (0, 4));
    assert_eq!(image.reachable_states(&first).unwrap(), vec![0]);
}

/// What F55 does on a new game with nothing answered by the game: the
/// instances it starts, the statements they issue and the events they wait
/// on. Printed for the notes; the counts are pinned.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_new_game_starts_the_scripts() {
    use ffe_core::script::engine::{Engine, Host};
    #[derive(Default)]
    struct Recorder {
        log: Vec<String>,
    }
    impl Host for Recorder {
        fn query(&mut self, token: u8, args: &[i32]) -> i32 {
            self.log.push(format!("query {token:#x} {args:?}"));
            0
        }
        fn statement(&mut self, opcode: u8, args: &[i32], text: &[u8]) -> i32 {
            self.log.push(format!(
                "statement {opcode} {args:?} {}",
                String::from_utf8_lossy(text)
            ));
            0
        }
        fn assign(&mut self, token: u8, args: &[i32], value: i32) -> Option<(i32, i32, i32)> {
            self.log
                .push(format!("assign {token:#x} {args:?} = {value}"));
            None
        }
    }
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let image = Image::new(cache.asset("base:script/missions").unwrap()).unwrap();
    let mut engine = Engine::new(&image);
    let mut host = Recorder::default();
    engine.new_game(&image, &mut host).unwrap();
    let live: Vec<_> = engine.live().collect();
    println!("live {live:?}");
    println!("subscriptions {}", engine.subscription_count());
    for line in &host.log {
        println!("{line}");
    }
    assert_eq!((live.len(), engine.subscription_count()), (20, 94));
    assert!(host.log[0].ends_with("Imperial Herald"));
}

/// The scripts create the journals and F68/F90 publish their issues: a new
/// game, then 120 days of daily messages, with the shared pair standing in
/// for the game's draws.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_scripts_publish_the_journals() {
    use ffe_core::journal::{decode_stories, Journal};
    use ffe_core::script::engine::Host;
    use ffe_core::script::run::{Game, Scripts};
    struct Draws {
        random: ffe_core::npc::Random,
        notices: Vec<String>,
    }
    impl Host for Draws {}
    impl Game for Draws {
        fn bounded(&mut self, bound: u32) -> u32 {
            self.random.bounded(bound)
        }
        fn advance(&mut self) -> i32 {
            self.random.advance() as i32
        }
        fn post(&mut self, text: &str) {
            self.notices.push(text.to_owned());
        }
    }
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let image = Image::new(cache.asset("base:script/missions").unwrap()).unwrap();
    let stories = decode_stories(cache.asset("base:text/journal-stories").unwrap()).unwrap();
    assert_eq!(stories.len(), 170);
    let archive = Journal::new(cache.asset("base:text/journal").unwrap()).unwrap();
    let mut game = Draws {
        random: ffe_core::npc::Random::start(),
        notices: Vec::new(),
    };
    let mut scripts = Scripts::new(&image);
    let start = 0x121cf7 + 1200;
    scripts
        .new_game(&image, &stories, &mut game, start, true)
        .unwrap();
    let names: Vec<String> = scripts
        .library
        .journals
        .iter()
        .map(|journal| String::from_utf8_lossy(&journal.name).into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "Imperial Herald",
            "Federal Times",
            "RIG",
            "Frontier News",
            "Universal Scientist"
        ]
    );
    for day in start + 1..start + 120 {
        scripts
            .daily(&image, &stories, &mut game, day, true)
            .unwrap();
    }
    for (slot, journal) in scripts.library.journals.iter().enumerate() {
        let issue = scripts.library.shown(slot);
        println!(
            "{}: next {} shown {issue:?}",
            names[slot],
            journal.next_issue - start
        );
        for article in issue.iter().take(2) {
            let headline = archive.article(*article as usize).unwrap().headline;
            println!("   {}", String::from_utf8_lossy(headline));
        }
    }
    println!(
        "{} notices, first: {:?}",
        game.notices.len(),
        game.notices.first()
    );
    assert_eq!(game.notices.len(), 20);
    assert!(scripts
        .library
        .journals
        .iter()
        .all(|j| j.next_issue == start + 140));
}
