use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_economy_player::interval::Player;
use game_language::Meaning;
use game_lots::{Movement, Place, Transaction};
use std::path::Path;

fn scene(quantum: u64, case: &str) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text =
        std::fs::read_to_string(root.join("content/scenarios/economy-personal-written-works.toml"))
            .unwrap()
            .replace("tick_millis = 900000", &format!("tick_millis = {quantum}"));
    let path = std::env::temp_dir().join(format!(
        "legacy-written-works-{}-{case}-{quantum}.toml",
        std::process::id()
    ));
    std::fs::write(&path, text).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    loaded.unwrap()
}

#[test]
fn a_work_has_real_custody_and_each_reader_interprets_marks_independently() {
    let mut host = scene(900000, "custody");
    let choices = || vec![vec![0, 1], vec![1, 2]];
    let mut author = Player::new(&mut host, 1).unwrap();
    assert!(author.compose(vec![vec![0, 1], vec![2, 2]]).is_err());
    let item = author.compose(choices()).unwrap().unwrap();
    assert!(author.compose(choices()).unwrap().is_none());
    assert_eq!(author.view().unwrap().written_works, [item]);
    let explained = author.observe_written_work(item).unwrap();
    assert_eq!(explained.forms[0].meanings, [Meaning::Place(1)]);
    assert_eq!(explained.forms[1].meanings, [Meaning::Person(2)]);
    assert!(
        Player::new(&mut host, 2)
            .unwrap()
            .observe_written_work(item)
            .is_err()
    );
    assert_eq!(host.advance_continuous(31, &[]).unwrap().failure, None);
    let good = host.snapshot().writing.unwrap().page;
    assert!(
        host.transfer_goods(&[Transaction {
            id: 1,
            movements: vec![Movement {
                id: 1,
                from: Place::Person(1),
                to: Some(Place::Person(2)),
                good,
                quantity: 1,
                batch: None,
                item: Some(item),
            }],
            creations: Vec::new()
        }])
        .unwrap()
        .transactions[0]
            .accepted
    );
    host = CoreHost::resume(
        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap()).unwrap(),
    )
    .unwrap();
    let reader = Player::new(&mut host, 2).unwrap();
    assert_eq!(reader.view().unwrap().written_works, [item]);
    let interpreted = reader.observe_written_work(item).unwrap();
    assert_eq!(
        (
            interpreted.forms[0].marks.clone(),
            interpreted.forms[1].marks.clone()
        ),
        (vec![91], vec![92])
    );
    assert_eq!(interpreted.forms[0].meanings, [Meaning::Person(1)]);
    assert!(interpreted.forms[1].meanings.is_empty());
}

#[test]
fn selected_surfaces_consume_only_their_materials_and_remain_with_the_work() {
    let mut host = scene(900000, "surfaces");
    let work = || vec![vec![0, 1], vec![1, 2]];
    let surface = host.snapshot().writing.unwrap().surfaces.clone();
    let [clay, bark] = surface.as_slice() else {
        panic!("expected a clay tablet and a bark sheet");
    };
    let mut author = Player::new(&mut host, 1).unwrap();
    let (paper, available) = author.writing_surfaces().unwrap();
    assert_eq!(available, [paper, clay.page, bark.page]);
    assert!(author.choose_writing_surface(Some(u32::MAX)).is_err());
    author.choose_writing_surface(Some(bark.page)).unwrap();
    let original = author.view().unwrap().possessions;
    assert!(author.compose(work()).unwrap().is_none());
    assert_eq!(author.view().unwrap().possessions, original);
    author.choose_writing_surface(Some(clay.page)).unwrap();
    let tablet = author.compose(work()).unwrap().unwrap();
    assert_eq!(
        author.observe_written_work(tablet).unwrap().surface,
        Some(clay.page)
    );
    assert!(
        !author
            .view()
            .unwrap()
            .possessions
            .iter()
            .any(|lot| lot.good == clay.blank.0)
    );
    assert!(
        author
            .view()
            .unwrap()
            .possessions
            .iter()
            .any(|lot| lot.good == bark.blank.0)
    );
    assert_eq!(host.advance_continuous(31, &[]).unwrap().failure, None);
    let pigment = bark.pigment.unwrap().0;
    assert!(
        host.transfer_goods(&[Transaction {
            id: 5,
            movements: vec![
                Movement {
                    id: 1,
                    from: Place::Person(1),
                    to: Some(Place::Person(2)),
                    good: clay.page,
                    quantity: 1,
                    batch: None,
                    item: Some(tablet)
                },
                Movement {
                    id: 2,
                    from: Place::Person(2),
                    to: Some(Place::Person(1)),
                    good: pigment,
                    quantity: 1,
                    batch: None,
                    item: None
                },
            ],
            creations: Vec::new(),
        }])
        .unwrap()
        .transactions[0]
            .accepted
    );
    host = CoreHost::resume(
        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap()).unwrap(),
    )
    .unwrap();
    let reader = Player::new(&mut host, 2).unwrap();
    let interpreted = reader.observe_written_work(tablet).unwrap();
    assert_eq!(interpreted.surface, Some(clay.page));
    assert_eq!(interpreted.forms[0].meanings, [Meaning::Person(1)]);
    let mut author = Player::new(&mut host, 1).unwrap();
    assert_eq!(author.writing_surfaces().unwrap().0, clay.page);
    author.choose_writing_surface(Some(bark.page)).unwrap();
    let sheet = author.compose(work()).unwrap().unwrap();
    assert_eq!(
        author.observe_written_work(sheet).unwrap().surface,
        Some(bark.page)
    );
    for good in [bark.blank.0, pigment] {
        assert!(
            !author
                .view()
                .unwrap()
                .possessions
                .iter()
                .any(|lot| lot.good == good)
        );
    }
    author.choose_writing_surface(None).unwrap();
    assert_eq!(author.writing_surfaces().unwrap().0, paper);
    let page = author.compose(work()).unwrap().unwrap();
    assert_eq!(author.observe_written_work(page).unwrap().surface, None);
    let mut invalid = host.snapshot();
    let pages = &mut invalid.writing.as_mut().unwrap().pages;
    std::sync::Arc::make_mut(pages)
        .iter_mut()
        .find(|page| page.item == sheet)
        .unwrap()
        .surface = Some(clay.page);
    assert!(CoreHost::resume(invalid).is_err());
}

#[test]
fn personal_leisure_and_authorship_preferences_pick_different_outcomes() {
    let run = |workers, quantum, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scene(quantum, "autonomy");
                let mut works = Vec::new();
                let mut songs = Vec::new();
                for &chunk in chunks {
                    let run = host.advance_continuous(chunk, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    for event in run.events {
                        match event {
                            CoreRunEvent::WrittenWorks(rows) => works.extend(rows),
                            CoreRunEvent::Pastimes(rows) => songs.extend(rows),
                            _ => (),
                        }
                    }
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                let owner = host.snapshot().writing.unwrap();
                assert_eq!(owner.pages.len(), 1);
                assert!(owner.compositions[0].completed);
                assert!(!owner.compositions[1].completed);
                assert_eq!(works.len(), 1);
                assert_eq!((works[0].person, works[0].at_millis), (3, 0));
                assert_eq!(works[0].surface, Some(owner.surfaces[0].page));
                assert_eq!(owner.pages[0].surface, works[0].surface);
                assert!(
                    songs
                        .iter()
                        .any(|row| row.person == 4 && row.at_millis == 0)
                );
                assert!(
                    !songs
                        .iter()
                        .any(|row| row.person == 3 && row.at_millis == 0)
                );
                (works, songs, owner.pages[0].work.clone())
            })
    };
    assert_eq!(run(1, 900000, &[3001]), run(6, 719, &[27, 901, 2073]));
}
