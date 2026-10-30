//! Personal choices about reproduction and partnerships.
use game_parallel_fused_physical::Kernel;
pub fn handle(engine: &mut Kernel, line: &str) -> Result<bool, String> {
    let mut words = line.split_whitespace();
    match words.next() {
        Some("partnership-preference") => {
            let person = super::number(&mut words, "person")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("expected enable or disable".into()),
            };
            super::end(&mut words)?;
            engine.set_partnership_reconsideration(person, enabled)?;
            println!("partnership-preference person={person} enabled={enabled}");
        }
        Some("reproduction") => {
            let person = super::number(&mut words, "person")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("expected enable or disable".into()),
            };
            super::end(&mut words)?;
            engine.set_reproduction(person, enabled)?;
            println!("reproduction person={person} enabled={enabled}");
        }
        Some("leave-partnership") => {
            let person = super::number(&mut words, "person")?;
            super::end(&mut words)?;
            engine.leave_partnership(person)?;
            println!("partnership left by person={person}");
        }
        _ => return Ok(false),
    }
    Ok(true)
}
