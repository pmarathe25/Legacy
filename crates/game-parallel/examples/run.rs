use game_parallel::World;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "content/parallel/example.toml".into());
    let mut world = World::from_toml(&std::fs::read_to_string(path)?)?;
    for _ in 0..100 {
        world.step()?;
    }
    println!("{}", serde_json::to_string_pretty(&world)?);
    Ok(())
}
